mod shift;
mod conv;

use rustpython_vm::{VirtualMachine, builtins::PyModule, PyRef};

/// Create the ops module
pub fn make_ops_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.ops", vm.ctx.new_dict(), None);
    
    module.set_attr("shift", vm.new_function("shift", shift::shift), vm).unwrap();
    module.set_attr("convolve", vm.new_function("convolve", conv::convolve), vm).unwrap();
    module.set_attr("convolve_depthwise", vm.new_function("convolve_depthwise", conv::convolve_depthwise), vm).unwrap();
    module.set_attr("convolve_image", vm.new_function("convolve_image", conv::convolve_image), vm).unwrap();
    module.set_attr("convolve_depthwise_image", vm.new_function("convolve_depthwise_image", conv::convolve_depthwise_image), vm).unwrap();
    
    module
}

