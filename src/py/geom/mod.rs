pub mod rect;

use rustpython_vm::{PyRef, VirtualMachine, builtins::PyModule};

pub fn make_geom_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.geom", vm.ctx.new_dict(), None);
    let rect_module = rect::make_rect_module(vm);
    module.set_attr("rect", rect_module, vm).unwrap();
    module
}
