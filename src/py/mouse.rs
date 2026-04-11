//! `xos.mouse` — synthetic desktop input for remote streaming (mirrors `remote` kernel).

use crate::python_api::mesh::py_to_json;
use rustpython_vm::builtins::PyModule;
use rustpython_vm::function::FuncArgs;
use rustpython_vm::{PyRef, PyResult, VirtualMachine};

fn mouse_apply_remote_input(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let payload_obj = args
        .args
        .first()
        .ok_or_else(|| vm.new_type_error("apply_remote_input(payload_dict)".to_string()))?
        .clone();
    let payload = py_to_json(vm, payload_obj)?;
    crate::apps::remote::apply_remote_input_python(&payload);
    Ok(vm.ctx.none())
}

pub fn register_mouse(module: &PyRef<PyModule>, vm: &VirtualMachine) {
    let sub = vm.new_module("xos.mouse", vm.ctx.new_dict(), None);
    let _ = sub.set_attr(
        "apply_remote_input",
        vm.new_function("apply_remote_input", mouse_apply_remote_input),
        vm,
    );
    let _ = module.set_attr("mouse", sub, vm);
}
