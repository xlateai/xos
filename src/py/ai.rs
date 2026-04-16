use std::path::PathBuf;

use burn::tensor::DType;
use burn_store::{BurnpackStore, ModuleStore};
use rustpython_vm::{AsObject, PyObjectRef, PyRef, PyResult, VirtualMachine, builtins::PyModule, function::FuncArgs};

fn to_py_err(vm: &VirtualMachine, msg: impl Into<String>) -> rustpython_vm::builtins::PyBaseExceptionRef {
    vm.new_runtime_error(msg.into())
}

fn parse_bool_flag(obj: Option<&PyObjectRef>, vm: &VirtualMachine) -> PyResult<bool> {
    match obj {
        Some(v) => v.clone().try_into_value(vm),
        None => Ok(false),
    }
}

fn parse_usize_flag(obj: Option<&PyObjectRef>, default: usize, vm: &VirtualMachine) -> PyResult<usize> {
    match obj {
        Some(v) => {
            let n: i64 = v.clone().try_into_value(vm)?;
            if n < 0 {
                Err(vm.new_value_error("max_values must be >= 0".to_string()))
            } else {
                Ok(n as usize)
            }
        }
        None => Ok(default),
    }
}

fn parse_string_arg(obj: Option<&PyObjectRef>, default: &str, vm: &VirtualMachine) -> PyResult<String> {
    match obj {
        Some(v) => v.clone().try_into_value(vm),
        None => Ok(default.to_string()),
    }
}

fn summarize(values: &[f32]) -> (usize, usize, usize, f32, f32, f32) {
    let mut finite = 0usize;
    let mut nan = 0usize;
    let mut inf = 0usize;
    let mut min_v = f32::INFINITY;
    let mut max_v = f32::NEG_INFINITY;
    let mut sum = 0.0f64;
    for &v in values {
        if v.is_finite() {
            finite += 1;
            min_v = min_v.min(v);
            max_v = max_v.max(v);
            sum += v as f64;
        } else if v.is_nan() {
            nan += 1;
        } else {
            inf += 1;
        }
    }
    let mean = if finite > 0 {
        (sum / finite as f64) as f32
    } else {
        f32::NAN
    };
    (finite, nan, inf, min_v, mean, max_v)
}

fn resolve_weights_path(model: &str, override_path: Option<String>) -> Result<PathBuf, String> {
    if let Some(p) = override_path {
        return Ok(PathBuf::from(p));
    }
    let home = std::env::var("HOME").map_err(|_| "HOME is not set".to_string())?;
    let root = PathBuf::from(home)
        .join(".xos")
        .join("models")
        .join("whisper")
        .join(model);
    let f32 = root.join(format!("{model}.bpk"));
    if f32.is_file() {
        return Ok(f32);
    }
    let f16 = root.join(format!("{model}-f16.bpk"));
    if f16.is_file() {
        return Ok(f16);
    }
    Err(format!(
        "weights not found: {} or {}",
        f32.display(),
        f16.display()
    ))
}

fn whisper_load_payload(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let av = args.args;
    let model = parse_string_arg(av.first(), "tiny", vm)?;
    let full_values = parse_bool_flag(av.get(1), vm)?;
    let max_values = parse_usize_flag(av.get(2), 128, vm)?;
    let override_path = if let Some(v) = av.get(3) {
        let s: String = v.clone().try_into_value(vm)?;
        if s.trim().is_empty() { None } else { Some(s) }
    } else {
        None
    };

    let weights_path = resolve_weights_path(&model, override_path).map_err(|e| to_py_err(vm, e))?;
    let weights_s = weights_path
        .to_str()
        .ok_or_else(|| to_py_err(vm, format!("invalid utf-8 path: {}", weights_path.display())))?;

    let mut store = BurnpackStore::from_file(weights_s);
    let snapshots = store
        .get_all_snapshots()
        .map_err(|e| to_py_err(vm, format!("failed to read snapshots: {e}")))?;

    let mut params_vec: Vec<PyObjectRef> = Vec::with_capacity(snapshots.len());
    for (name, snapshot) in snapshots {
        let entry = vm.ctx.new_dict();
        entry
            .set_item("name", vm.ctx.new_str(name.clone()).into(), vm)
            .map_err(|e| to_py_err(vm, format!("dict set name: {e:?}")))?;
        entry
            .set_item(
                "dtype",
                vm.ctx.new_str(format!("{:?}", snapshot.dtype)).into(),
                vm,
            )
            .map_err(|e| to_py_err(vm, format!("dict set dtype: {e:?}")))?;
        let shape_items: Vec<PyObjectRef> = snapshot
            .shape
            .iter()
            .map(|&d| vm.ctx.new_int(d as i64).into())
            .collect();
        entry
            .set_item("shape", vm.ctx.new_list(shape_items).into(), vm)
            .map_err(|e| to_py_err(vm, format!("dict set shape: {e:?}")))?;
        entry
            .set_item(
                "numel",
                vm.ctx.new_int(snapshot.shape.num_elements() as i64).into(),
                vm,
            )
            .map_err(|e| to_py_err(vm, format!("dict set numel: {e:?}")))?;

        if matches!(snapshot.dtype, DType::F16 | DType::F32 | DType::BF16) {
            let data = snapshot
                .to_data()
                .map_err(|e| to_py_err(vm, format!("materialize {name}: {e}")))?;
            let vals = data
                .convert::<f32>()
                .to_vec::<f32>()
                .map_err(|e| to_py_err(vm, format!("to_vec {name}: {e}")))?;
            let (finite, nan, inf, min_v, mean_v, max_v) = summarize(&vals);
            let stats = vm.ctx.new_dict();
            stats
                .set_item("finite", vm.ctx.new_int(finite as i64).into(), vm)
                .ok();
            stats.set_item("nan", vm.ctx.new_int(nan as i64).into(), vm).ok();
            stats.set_item("inf", vm.ctx.new_int(inf as i64).into(), vm).ok();
            stats
                .set_item("min", vm.ctx.new_float(min_v as f64).into(), vm)
                .ok();
            stats
                .set_item("mean", vm.ctx.new_float(mean_v as f64).into(), vm)
                .ok();
            stats
                .set_item("max", vm.ctx.new_float(max_v as f64).into(), vm)
                .ok();
            entry.set_item("stats", stats.into(), vm).ok();

            let take = if full_values {
                vals.len()
            } else {
                max_values.min(vals.len())
            };
            let py_vals: Vec<PyObjectRef> = vals
                .iter()
                .take(take)
                .map(|v| vm.ctx.new_float(*v as f64).into())
                .collect();
            entry
                .set_item("values", vm.ctx.new_list(py_vals).into(), vm)
                .ok();
            entry
                .set_item(
                    "values_truncated",
                    vm.ctx.new_bool(!full_values && vals.len() > max_values).into(),
                    vm,
                )
                .ok();
        }

        params_vec.push(entry.into());
    }

    let params = vm.ctx.new_list(params_vec);
    let payload = vm.ctx.new_dict();
    payload
        .set_item("weights_file", vm.ctx.new_str(weights_s).into(), vm)
        .ok();
    payload
        .set_item(
            "parameter_count",
            vm.ctx.new_int(snapshots.len() as i64).into(),
            vm,
        )
        .ok();
    payload
        .set_item("parameters", params.as_object().to_owned(), vm)
        .ok();
    Ok(payload.into())
}

pub fn make_ai_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let ai = vm.new_module("xos.ai", vm.ctx.new_dict(), None);
    let whisper = vm.new_module("xos.ai.whisper", vm.ctx.new_dict(), None);
    whisper
        .set_attr("_load_payload", vm.new_function("_load_payload", whisper_load_payload), vm)
        .ok();

    let scope = vm.new_scope_with_builtins();
    if let Ok(loader) = whisper.get_attr("_load_payload", vm) {
        scope.globals.set_item("_load_payload", loader, vm).ok();
    }

    let glue = r#"
class _WhisperModel:
    def __init__(self, payload):
        self._payload = payload
    def named_parameters(self):
        for p in self._payload["parameters"]:
            yield p["name"], p
    @property
    def parameters(self):
        return self._payload["parameters"]
    @property
    def parameter_count(self):
        return self._payload["parameter_count"]
    @property
    def weights_file(self):
        return self._payload["weights_file"]

def load(model="tiny", full_values=False, max_values=128, weights_path=None):
    payload = _load_payload(model, full_values, max_values, weights_path)
    return _WhisperModel(payload)
"#;
    if vm
        .run_code_string(scope.clone(), glue, "<xos.ai.whisper>".to_string())
        .is_ok()
    {
        if let Ok(load_fn) = scope.globals.get_item("load", vm) {
            whisper.set_attr("load", load_fn, vm).ok();
        }
        if let Ok(cls) = scope.globals.get_item("_WhisperModel", vm) {
            whisper.set_attr("WhisperModel", cls, vm).ok();
        }
    }

    ai.set_attr("whisper", whisper, vm).ok();
    ai
}

