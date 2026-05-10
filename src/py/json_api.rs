//! `xos.json` — `dumps` / `loads` with native support for [`builtins.Frame`](crate::python_api::engine::pyapp).

use crate::python_api::json_codec::{json_value_to_py, py_to_json_value};
use rustpython_vm::builtins::PyModule;
use rustpython_vm::function::FuncArgs;
use rustpython_vm::AsObject;
use rustpython_vm::{PyRef, PyResult, VirtualMachine};

fn json_dumps(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let obj = args
        .args
        .first()
        .cloned()
        .or_else(|| args.kwargs.get("obj").cloned())
        .ok_or_else(|| vm.new_type_error("dumps(obj) missing required argument".to_string()))?;

    let value = py_to_json_value(vm, obj, 0).map_err(|e| e)?;
    let s = serde_json::to_string(&value)
        .map_err(|err| vm.new_runtime_error(format!("xos.json.dumps: serialize error: {err}")))?;
    Ok(vm.ctx.new_str(s.as_str()).into())
}

fn json_loads(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let text: String = args
        .args
        .first()
        .cloned()
        .or_else(|| args.kwargs.get("s").cloned())
        .ok_or_else(|| vm.new_type_error("loads(s) missing string argument".to_string()))?
        .try_into_value(vm)?;

    let v: serde_json::Value = serde_json::from_str(text.trim()).map_err(|err| {
        vm.new_value_error(format!("xos.json.loads: invalid JSON ({err})"))
    })?;
    json_value_to_py(vm, &v)
}

pub fn register_json(parent: &PyRef<PyModule>, vm: &VirtualMachine) {
    let sub = vm.new_module("xos.json", vm.ctx.new_dict(), None);
    let _ = sub.set_attr("dumps", vm.new_function("dumps", json_dumps), vm);
    let _ = sub.set_attr("loads", vm.new_function("loads", json_loads), vm);
    let _ = parent.set_attr("json", sub.as_object().to_owned(), vm);
}
