#[cfg(feature = "python")]
use rustpython_vm::{PyResult, VirtualMachine, PyObjectRef, builtins::PyModule, PyRef};

/// The xos.hello() function
fn hello(_args: Vec<PyObjectRef>, _vm: &VirtualMachine) -> PyResult {
    println!("hello from xos module");
    Ok(_vm.ctx.none())
}

/// Create the xos module
pub fn make_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos", vm.ctx.new_dict(), None);
    
    // Add the hello function to the module
    let hello_func = vm.new_function("hello", hello);
    module.set_attr("hello", hello_func, vm).unwrap();
    
    module
}

