mod shift;
mod convolution;

use rustpython_vm::{VirtualMachine, builtins::PyModule, PyRef};

/// Create the ops module
pub fn make_ops_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.ops", vm.ctx.new_dict(), None);
    
    module.set_attr("shift", vm.new_function("shift", shift::shift), vm).unwrap();
    module.set_attr("convolve", vm.new_function("convolve", convolution::convolve), vm).unwrap();
    
    module
}

