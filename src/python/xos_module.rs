#[cfg(feature = "python")]
use rustpython_vm::{PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs};

/// The xos.hello() function
fn hello(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    println!("hello from xos module");
    Ok(vm.ctx.none())
}

/// xos.get_frame_buffer() - returns the frame buffer dimensions and a placeholder
/// In the future, this will return actual frame buffer access
fn get_frame_buffer(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    // For now, return a dict with width, height, and placeholder buffer
    let dict = vm.ctx.new_dict();
    dict.set_item("width", vm.ctx.new_int(800).into(), vm)?;
    dict.set_item("height", vm.ctx.new_int(600).into(), vm)?;
    dict.set_item("buffer", vm.ctx.new_list(vec![]).into(), vm)?;
    Ok(dict.into())
}

/// xos.get_mouse() - returns mouse position
fn get_mouse(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let dict = vm.ctx.new_dict();
    dict.set_item("x", vm.ctx.new_float(0.0).into(), vm)?;
    dict.set_item("y", vm.ctx.new_float(0.0).into(), vm)?;
    dict.set_item("down", vm.ctx.new_bool(false).into(), vm)?;
    Ok(dict.into())
}

/// xos.print() - print to xos console
fn xos_print(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let msg: String = args.bind(vm)?;
    println!("[xos] {}", msg);
    Ok(vm.ctx.none())
}

/// Create the xos module with Application base class
pub fn make_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos", vm.ctx.new_dict(), None);
    
    // Add functions to the module
    module.set_attr("hello", vm.new_function("hello", hello), vm).unwrap();
    module.set_attr("get_frame_buffer", vm.new_function("get_frame_buffer", get_frame_buffer), vm).unwrap();
    module.set_attr("get_mouse", vm.new_function("get_mouse", get_mouse), vm).unwrap();
    module.set_attr("print", vm.new_function("print", xos_print), vm).unwrap();
    
    // Add the random submodule
    let random_module = crate::python::random::random::make_random_module(vm);
    module.set_attr("random", random_module, vm).unwrap();
    
    // Add the rasterizer submodule
    let rasterizer_module = crate::python::rasterizer::make_rasterizer_module(vm);
    module.set_attr("rasterizer", rasterizer_module, vm).unwrap();
    
    // Define the Application base class in Python
    let application_class_code = crate::python::engine::pyapp::APPLICATION_CLASS_CODE;
    
    // Execute the Application class definition
    let scope = vm.new_scope_with_builtins();
    if let Err(e) = vm.run_code_string(scope.clone(), application_class_code, "<xos_module>".to_string()) {
        eprintln!("Failed to create Application class: {:?}", e);
    }
    
    // Get the Application class and _FrameWrapper from the scope and add them to the module
    if let Ok(app_class) = scope.globals.get_item("Application", vm) {
        module.set_attr("Application", app_class, vm).unwrap();
    }
    if let Ok(frame_wrapper) = scope.globals.get_item("_FrameWrapper", vm) {
        module.set_attr("_FrameWrapper", frame_wrapper.clone(), vm).unwrap();
        // Also add to builtins so pyapp can access it
        let _ = vm.builtins.set_attr("_FrameWrapper", frame_wrapper, vm);
    }
    
    module
}

