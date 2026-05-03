//! `xos.clipboard` — system pasteboard (`crate::clipboard`).

use rustpython_vm::{
    PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs,
};

fn clipboard_get(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    if !args.args.is_empty() {
        return Err(vm.new_type_error("clipboard.get() takes no arguments".to_string()));
    }
    let s = crate::clipboard::get_contents().unwrap_or_default();
    Ok(vm.ctx.new_str(s.as_str()).into())
}

fn clipboard_set(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    if args.args.len() != 1 {
        return Err(vm.new_type_error("clipboard.set(text) takes one string".to_string()));
    }
    let s: String = args.args[0].clone().try_into_value(vm)?;
    let _ = crate::clipboard::set_contents(&s);
    Ok(vm.ctx.none())
}

pub fn make_clipboard_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let m = vm.new_module("xos.clipboard", vm.ctx.new_dict(), None);
    m.set_attr("get", vm.new_function("get", clipboard_get), vm)
        .unwrap();
    m.set_attr("set", vm.new_function("set", clipboard_set), vm)
        .unwrap();
    m
}
