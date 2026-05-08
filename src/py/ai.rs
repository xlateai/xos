#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

#[cfg(not(target_arch = "wasm32"))]
use burn::tensor::DType;
#[cfg(not(target_arch = "wasm32"))]
use burn_store::{BurnpackStore, ModuleStore};
#[cfg(not(target_arch = "wasm32"))]
use rustpython_vm::AsObject;
use rustpython_vm::{PyObjectRef, PyRef, PyResult, VirtualMachine, builtins::PyModule, function::FuncArgs};

use crate::tensor::tensor::tensor_flat_data_list;

/// Mono `f32` samples — same contract as in-tree `whisper_burn::transcribe`: one contiguous buffer,
/// values typically in ~`[-1, 1]`, `sample_rate` Hz (Whisper expects 16 kHz).
fn waveform_vec_from_py(obj: &PyObjectRef, vm: &VirtualMachine) -> PyResult<Vec<f32>> {
    match tensor_flat_data_list(obj, vm) {
        Ok(v) if !v.is_empty() => Ok(v),
        Ok(_) => Err(vm.new_value_error(
            "waveform is empty: need non-empty float32 mono PCM (e.g. xos.audio.load(path, 16000))"
                .to_string(),
        )),
        Err(_) => obj.clone().try_into_value::<Vec<f32>>(vm),
    }
}

fn to_py_err(vm: &VirtualMachine, msg: impl Into<String>) -> rustpython_vm::builtins::PyBaseExceptionRef {
    vm.new_runtime_error(msg.into())
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_bool_flag(obj: Option<&PyObjectRef>, vm: &VirtualMachine) -> PyResult<bool> {
    match obj {
        Some(v) => v.clone().try_into_value(vm),
        None => Ok(false),
    }
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
fn resolve_weights_path(model: &str, override_path: Option<String>) -> Result<PathBuf, String> {
    if let Some(p) = override_path {
        return Ok(PathBuf::from(p));
    }
    let model_trim = model.trim();
    let (dir_name, require_f16) = if let Some(stem) = model_trim.strip_suffix("-f16") {
        if stem.is_empty() {
            return Err("invalid whisper model: use e.g. tiny-f16".to_string());
        }
        (stem, true)
    } else {
        (model_trim, false)
    };
    // Same tree as `xos path --data` (`auth_data_dir()`): `%LOCALAPPDATA%/xos` on Windows, `~/.xos` elsewhere.
    let new_root =
        crate::auth::whisper_model_backend_cache_dir(dir_name, "burn").map_err(|e| e.to_string())?;
    let legacy_transcription = crate::auth::auth_data_dir()
        .map_err(|e| e.to_string())?
        .join("models")
        .join("transcription")
        .join("burn")
        .join(dir_name);
    let legacy_root = crate::auth::whisper_model_cache_dir(dir_name).map_err(|e| e.to_string())?;
    let pick_pack = |root: &PathBuf| -> Option<PathBuf> {
        let f32 = root.join(format!("{dir_name}.bpk"));
        let f16 = root.join(format!("{dir_name}-f16.bpk"));
        if require_f16 {
            return f16.is_file().then_some(f16);
        }
        if f32.is_file() {
            return Some(f32);
        }
        f16.is_file().then_some(f16)
    };
    if let Some(p) = pick_pack(&new_root) {
        return Ok(p);
    }
    if let Some(p) = pick_pack(&legacy_transcription) {
        return Ok(p);
    }
    if let Some(p) = pick_pack(&legacy_root) {
        return Ok(p);
    }
    let f32 = new_root.join(format!("{dir_name}.bpk"));
    let f16 = new_root.join(format!("{dir_name}-f16.bpk"));
    if require_f16 {
        return Err(format!(
            "F16 Whisper weights not found: {} (expected for model id ending in -f16; also checked {})",
            f16.display(),
            legacy_root.join(format!("{dir_name}-f16.bpk")).display()
        ));
    }
    Err(format!(
        "weights not found under {} or {} or {} (expected {} or {})",
        new_root.display(),
        legacy_transcription.display(),
        legacy_root.display(),
        f32.display(),
        f16.display()
    ))
}

#[cfg(not(target_arch = "wasm32"))]
fn whisper_load_payload(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let av = args.args;
    let model = parse_string_arg(av.first(), "tiny", vm)?;
    let full_values = parse_bool_flag(av.get(1), vm)?;
    let max_values = parse_usize_flag(av.get(2), 128, vm)?;
    let override_path: Option<String> = if let Some(v) = av.get(3) {
        v.clone().try_into_value(vm)?
    } else {
        None
    };
    let override_path = override_path.and_then(|s| {
        if s.trim().is_empty() {
            None
        } else {
            Some(s)
        }
    });

    // Match native transcription: populate ~/.xos/models/whisper/{tiny,small}-burn/ before opening burnpack.
    if override_path.is_none() {
        let model_trim = model.trim();
        let stem = model_trim.strip_suffix("-f16").unwrap_or(model_trim);
        if !stem.is_empty() {
            #[cfg(all(
                feature = "whisper_burn",
                not(target_arch = "wasm32"),
                not(target_os = "ios")
            ))]
            {
                crate::ai::transcription::ensure_burn_whisper_artifacts_for_load(stem)
                    .map_err(|e| to_py_err(vm, e))?;
            }
        }
    }

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

#[cfg(target_arch = "wasm32")]
fn whisper_load_payload(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    Err(to_py_err(
        vm,
        "Burn weight payload inspection is unavailable on wasm builds",
    ))
}

fn whisper_forward_native(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let av = args.args;
    let model = parse_string_arg(av.first(), "tiny", vm)?;
    let waveform: Vec<f32> = match av.get(1) {
        Some(v) => waveform_vec_from_py(v, vm)?,
        None => {
            return Err(vm.new_type_error(
                "_forward_native(model, waveform, sample_rate=16000) requires waveform".to_string(),
            ));
        }
    };
    let sample_rate: i64 = if let Some(v) = av.get(2) {
        v.clone().try_into_value(vm)?
    } else {
        16_000
    };
    if sample_rate <= 0 {
        return Err(vm.new_value_error("sample_rate must be > 0".to_string()));
    }

    let backend_s = parse_string_arg(av.get(3), "burn", vm)?;
    let backend = crate::ai::transcription::WhisperBackend::from_str(&backend_s).ok_or_else(|| {
        vm.new_value_error(format!(
            "unknown whisper backend '{backend_s}' (use 'burn' or 'ct2')"
        ))
    })?;

    let text = crate::ai::transcription::transcribe_waveform_once(
        Some(&model),
        &waveform,
        sample_rate as u32,
        backend,
    )
    .map_err(|e| to_py_err(vm, e))?;
    Ok(vm.ctx.new_str(text).into())
}

fn whisper_forward_layer_by_layer_native(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let av = args.args;
    let model = parse_string_arg(av.first(), "tiny", vm)?;
    let waveform: Vec<f32> = match av.get(1) {
        Some(v) => waveform_vec_from_py(v, vm)?,
        None => {
            return Err(vm.new_type_error(
                "_forward_layer_by_layer_native(model, waveform, sample_rate=16000) requires waveform"
                    .to_string(),
            ));
        }
    };
    let sample_rate: i64 = if let Some(v) = av.get(2) {
        v.clone().try_into_value(vm)?
    } else {
        16_000
    };
    if sample_rate <= 0 {
        return Err(vm.new_value_error("sample_rate must be > 0".to_string()));
    }

    let backend_s = parse_string_arg(av.get(3), "burn", vm)?;
    let backend = crate::ai::transcription::WhisperBackend::from_str(&backend_s).ok_or_else(|| {
        vm.new_value_error(format!(
            "unknown whisper backend '{backend_s}' (use 'burn' or 'ct2')"
        ))
    })?;

    let (text, steps) = crate::ai::transcription::transcribe_waveform_with_intermediates(
        Some(&model),
        &waveform,
        sample_rate as u32,
        backend,
    )
    .map_err(|e| to_py_err(vm, e))?;

    let out_steps: Vec<PyObjectRef> = steps
            .into_iter()
            .map(|s| {
                let d = vm.ctx.new_dict();
                match s.name {
                    Some(name) => d.set_item("name", vm.ctx.new_str(name).into(), vm).ok(),
                    None => d.set_item("name", vm.ctx.none(), vm).ok(),
                };
                let shape: Vec<PyObjectRef> = s
                    .shape
                    .into_iter()
                    .map(|v| vm.ctx.new_int(v as i64).into())
                    .collect();
                let prefix_len = s.values.len();
                let values: Vec<PyObjectRef> = s
                    .values
                    .into_iter()
                    .map(|v| vm.ctx.new_float(v as f64).into())
                    .collect();
                d.set_item("shape", vm.ctx.new_list(shape).into(), vm).ok();
                d.set_item("values", vm.ctx.new_list(values).into(), vm).ok();
                let stats = vm.ctx.new_dict();
                stats
                    .set_item("num_values", vm.ctx.new_int(prefix_len as i64).into(), vm)
                    .ok();
                if let Some(fs) = s.full_stats {
                    stats
                        .set_item("full_mean", vm.ctx.new_float(f64::from(fs.mean)).into(), vm)
                        .ok();
                    stats
                        .set_item("full_std", vm.ctx.new_float(f64::from(fs.std)).into(), vm)
                        .ok();
                    stats
                        .set_item("full_min", vm.ctx.new_float(f64::from(fs.min)).into(), vm)
                        .ok();
                    stats
                        .set_item("full_max", vm.ctx.new_float(f64::from(fs.max)).into(), vm)
                        .ok();
                }
                if let Some((ds, dam)) = s.device_preflight {
                    stats
                        .set_item("device_sum", vm.ctx.new_float(f64::from(ds)).into(), vm)
                        .ok();
                    stats
                        .set_item("device_abs_max", vm.ctx.new_float(f64::from(dam)).into(), vm)
                        .ok();
                }
                d.set_item("stats", stats.into(), vm).ok();
                d.into()
            })
            .collect();
    let payload = vm.ctx.new_dict();
    payload.set_item("text", vm.ctx.new_str(text).into(), vm).ok();
    payload
        .set_item("steps", vm.ctx.new_list(out_steps).into(), vm)
        .ok();
    Ok(payload.into())
}

pub fn make_ai_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let ai = vm.new_module("xos.ai", vm.ctx.new_dict(), None);
    let whisper = vm.new_module("xos.ai.whisper", vm.ctx.new_dict(), None);
    whisper
        .set_attr("_load_payload", vm.new_function("_load_payload", whisper_load_payload), vm)
        .ok();
    whisper
        .set_attr(
            "_forward_native",
            vm.new_function("_forward_native", whisper_forward_native),
            vm,
        )
        .ok();
    whisper
        .set_attr(
            "_forward_layer_by_layer_native",
            vm.new_function(
                "_forward_layer_by_layer_native",
                whisper_forward_layer_by_layer_native,
            ),
            vm,
        )
        .ok();

    let scope = vm.new_scope_with_builtins();
    if let Ok(loader) = whisper.get_attr("_load_payload", vm) {
        scope.globals.set_item("_load_payload", loader, vm).ok();
    }
    if let Ok(forward_native) = whisper.get_attr("_forward_native", vm) {
        scope
            .globals
            .set_item("_forward_native", forward_native, vm)
            .ok();
    }
    if let Ok(fwd_lbl) = whisper.get_attr("_forward_layer_by_layer_native", vm) {
        scope
            .globals
            .set_item("_forward_layer_by_layer_native", fwd_lbl, vm)
            .ok();
    }

    let glue = r#"
BURN = "burn"
CT2 = "ct2"

def _mk_parameter(payload):
    xos = __import__("xos")
    return xos.nn.Parameter(
        payload["name"],
        payload["shape"],
        payload["dtype"],
        payload.get("values", []),
        payload.get("stats", {}),
    )

def _waveform_to_list(x):
    if isinstance(x, (list, tuple)):
        out = []
        for v in x:
            out.append(float(v))
        return out
    if hasattr(x, "list"):
        try:
            vals = x.list()
            if isinstance(vals, (list, tuple)):
                return [float(v) for v in vals]
        except Exception:
            pass
    if hasattr(x, "_data"):
        d = x._data
        if isinstance(d, dict):
            raw = d.get("_data", None)
            if raw is None:
                raw = d.get("data", None)
            if isinstance(raw, (list, tuple)):
                if raw and isinstance(raw[0], (list, tuple)):
                    return [[float(v) for v in row] for row in raw]
                return [float(v) for v in raw]
        elif isinstance(d, (list, tuple)):
            if d and isinstance(d[0], (list, tuple)):
                return [[float(v) for v in row] for row in d]
            return [float(v) for v in d]
    return [float(x)]

def _flatten_batch_dim1(wave):
    # Shape (1, N) from xos often appears as one row: match whisper_burn's flat &[f32].
    if (
        len(wave) == 1
        and isinstance(wave[0], (list, tuple))
        and wave[0]
        and not isinstance(wave[0][0], (list, tuple))
    ):
        return [float(v) for v in wave[0]]
    return wave

class _WhisperCt2Model:
    """CTranslate2 backend: no Burnpack weights in Python; inference is native CT2."""
    def __init__(self, model):
        self._model = model
    def named_parameters(self):
        return iter(())
    def parameters(self):
        return []
    def get_parameter(self, name):
        return None
    @property
    def parameter_count(self):
        return 0
    @property
    def weights_file(self):
        return None
    def forward(self, x, sample_rate=16000):
        wave = _flatten_batch_dim1(_waveform_to_list(x))
        if wave and isinstance(wave[0], (list, tuple)):
            return [_forward_native(self._model, [float(v) for v in row], int(sample_rate), CT2) for row in wave]
        return _forward_native(self._model, [float(v) for v in wave], int(sample_rate), CT2)
    def forward_layer_by_layer(self, x, sample_rate=16000):
        raise NotImplementedError("forward_layer_by_layer requires backend=BURN")

class _WhisperModel:
    def __init__(self, payload):
        self._payload = payload
        self._model = payload.get("model", "tiny")
    def named_parameters(self):
        for p in self._payload["parameters"]:
            param = _mk_parameter(p)
            yield p["name"], param
    @property
    def parameters(self):
        return [_mk_parameter(p) for p in self._payload["parameters"]]
    def get_parameter(self, name):
        for p in self.parameters:
            if p.name == name:
                return p
        return None
    @property
    def parameter_count(self):
        return self._payload["parameter_count"]
    @property
    def weights_file(self):
        return self._payload["weights_file"]
    def forward(self, x, sample_rate=16000):
        wave = _flatten_batch_dim1(_waveform_to_list(x))
        if wave and isinstance(wave[0], (list, tuple)):
            return [_forward_native(self._model, [float(v) for v in row], int(sample_rate), BURN) for row in wave]
        return _forward_native(self._model, [float(v) for v in wave], int(sample_rate), BURN)
    def forward_layer_by_layer(self, x, sample_rate=16000):
        wave = _flatten_batch_dim1(_waveform_to_list(x))
        if wave and isinstance(wave[0], (list, tuple)):
            raise ValueError("forward_layer_by_layer currently supports only a single waveform")
        payload = _forward_layer_by_layer_native(
            self._model,
            [float(v) for v in wave],
            int(sample_rate),
            BURN,
        )
        for step in payload["steps"]:
            name = step.get("name", None)
            if name is None:
                yield None, payload.get("text", "")
                continue
            act = _mk_parameter({
                "name": name if name is not None else "output",
                "shape": step.get("shape", []),
                "dtype": "float32",
                "values": step.get("values", []),
                "stats": step.get("stats") or {},
            })
            yield name, act

def load(model="tiny", full_values=False, max_values=128, weights_path=None, backend=CT2):
    # model: tiny | small | tiny-f16 | small-f16 (Burn only: -f16 selects F16 burnpack + f16 compute)
    # backend: BURN (Burnpack + WGPU) or CT2 (CTranslate2); default CT2.
    if backend == CT2:
        return _WhisperCt2Model(model)
    if backend != BURN:
        raise ValueError("whisper.load backend must be BURN or CT2")
    payload = _load_payload(model, full_values, max_values, weights_path)
    payload["model"] = model
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
        if let Ok(cls) = scope.globals.get_item("_WhisperCt2Model", vm) {
            whisper.set_attr("WhisperCt2Model", cls, vm).ok();
        }
        if let Ok(v) = scope.globals.get_item("BURN", vm) {
            whisper.set_attr("BURN", v, vm).ok();
        }
        if let Ok(v) = scope.globals.get_item("CT2", vm) {
            whisper.set_attr("CT2", v, vm).ok();
        }
    }

    ai.set_attr("whisper", whisper, vm).ok();
    ai
}

