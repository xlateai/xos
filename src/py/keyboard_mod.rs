//! `xos.keyboard` — minimal hooks into the viewport on-screen keyboard (Python apps).

use rustpython_vm::{PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs};

fn toggle_onscreen(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    crate::python_api::viewport_keyboard_bridge::request_onscreen_keyboard_toggle();
    Ok(vm.ctx.none())
}

pub fn register_keyboard(module: &PyRef<PyModule>, vm: &VirtualMachine) {
    let sub = vm.new_module("xos.keyboard", vm.ctx.new_dict(), None);
    sub
        .set_attr(
            "toggle_onscreen",
            vm.new_function("toggle_onscreen", toggle_onscreen),
            vm,
        )
        .unwrap();
    let _ = module.set_attr("keyboard", sub, vm);
}
