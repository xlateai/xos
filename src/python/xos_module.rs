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

/// xos.print() - alias to builtin print (no longer needed, kept for compatibility)
/// We'll set this to builtins.print in make_module instead

/// xos.sleep() - sleep for a number of seconds
/// NOTE: This blocks the main thread, so it's not recommended for use in the coder app
/// For periodic updates, use a viewport app with tick() instead
fn xos_sleep(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let seconds: f64 = args.bind(vm)?;
    let duration = std::time::Duration::from_secs_f64(seconds);
    std::thread::sleep(duration);
    Ok(vm.ctx.none())
}

/// Create the xos module with Application base class
pub fn make_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos", vm.ctx.new_dict(), None);
    
    // Add functions to the module
    module.set_attr("hello", vm.new_function("hello", hello), vm).unwrap();
    module.set_attr("get_frame_buffer", vm.new_function("get_frame_buffer", get_frame_buffer), vm).unwrap();
    module.set_attr("get_mouse", vm.new_function("get_mouse", get_mouse), vm).unwrap();
    
    // Make xos.print an alias to the builtin print function
    if let Ok(builtin_print) = vm.builtins.get_attr("print", vm) {
        module.set_attr("print", builtin_print, vm).unwrap();
    }
    
    module.set_attr("sleep", vm.new_function("sleep", xos_sleep), vm).unwrap();
    
    // Add the random submodule
    let random_module = crate::python::random::random::make_random_module(vm);
    module.set_attr("random", random_module, vm).unwrap();
    
    // Add the rasterizer submodule
    let rasterizer_module = crate::python::rasterizer::make_rasterizer_module(vm);
    module.set_attr("rasterizer", rasterizer_module, vm).unwrap();
    
    // Add the sensors submodule
    let sensors_module = crate::python::sensors::make_sensors_module(vm);
    module.set_attr("sensors", sensors_module, vm).unwrap();
    
    // Add the audio submodule
    let audio_module = crate::python::audio::make_audio_module(vm);
    module.set_attr("audio", audio_module, vm).unwrap();
    
    // Add the system submodule
    let system_module = crate::python::system::make_system_module(vm);
    module.set_attr("system", system_module, vm).unwrap();
    
    // Add the dialoguer submodule
    let dialoguer_module = crate::python::dialoguer::make_dialoguer_module(vm);
    module.set_attr("dialoguer", dialoguer_module, vm).unwrap();
    
    // Add the math submodule
    let math_module = crate::python::math::make_math_module(vm);
    module.set_attr("math", math_module, vm).unwrap();
    
    // Add the ops submodule
    let ops_module = crate::python::ops::make_ops_module(vm);
    module.set_attr("ops", ops_module, vm).unwrap();
    
    // Add the arrays submodule  
    let arrays_module = crate::python::arrays::make_arrays_module(vm);
    module.set_attr("array", arrays_module.get_attr("array", vm).unwrap(), vm).unwrap();
    module.set_attr("zeros", arrays_module.get_attr("zeros", vm).unwrap(), vm).unwrap();
    module.set_attr("ones", arrays_module.get_attr("ones", vm).unwrap(), vm).unwrap();
    
    // Add the data submodule
    let data_module = crate::python::data::make_data_module(vm);
    module.set_attr("data", data_module, vm).unwrap();
    
    // Add the ui submodule
    let ui_module = crate::python::ui::make_ui_module(vm);
    module.set_attr("ui", ui_module, vm).unwrap();
    
    // Add the dtypes module and expose dtype constants
    let dtypes_module = crate::python::dtypes::make_dtypes_module(vm);
    // Expose all dtype constants directly on xos module
    if let Ok(dtype) = dtypes_module.get_attr("float16", vm) {
        module.set_attr("float16", dtype, vm).ok();
    }
    if let Ok(dtype) = dtypes_module.get_attr("float32", vm) {
        module.set_attr("float32", dtype.clone(), vm).ok();
        module.set_attr("float", dtype, vm).ok();
    }
    if let Ok(dtype) = dtypes_module.get_attr("float64", vm) {
        module.set_attr("float64", dtype, vm).ok();
    }
    if let Ok(dtype) = dtypes_module.get_attr("int8", vm) {
        module.set_attr("int8", dtype, vm).ok();
    }
    if let Ok(dtype) = dtypes_module.get_attr("int16", vm) {
        module.set_attr("int16", dtype, vm).ok();
    }
    if let Ok(dtype) = dtypes_module.get_attr("int32", vm) {
        module.set_attr("int32", dtype.clone(), vm).ok();
        module.set_attr("int", dtype, vm).ok();
    }
    if let Ok(dtype) = dtypes_module.get_attr("int64", vm) {
        module.set_attr("int64", dtype, vm).ok();
    }
    if let Ok(dtype) = dtypes_module.get_attr("uint8", vm) {
        module.set_attr("uint8", dtype, vm).ok();
    }
    if let Ok(dtype) = dtypes_module.get_attr("uint16", vm) {
        module.set_attr("uint16", dtype, vm).ok();
    }
    if let Ok(dtype) = dtypes_module.get_attr("uint32", vm) {
        module.set_attr("uint32", dtype, vm).ok();
    }
    if let Ok(dtype) = dtypes_module.get_attr("uint64", vm) {
        module.set_attr("uint64", dtype, vm).ok();
    }
    if let Ok(dtype) = dtypes_module.get_attr("bool", vm) {
        module.set_attr("bool", dtype, vm).ok();
    }
    
    // Define the Application base class in Python
    let application_class_code = crate::python::engine::pyapp::APPLICATION_CLASS_CODE;
    
    // Execute the Application class definition
    let scope = vm.new_scope_with_builtins();
    if let Err(e) = vm.run_code_string(scope.clone(), application_class_code, "<xos_module>".to_string()) {
        eprintln!("Failed to create Application class: {:?}", e);
    }
    
    // Add _ArrayWrapper and _ArrayResult to builtins so they're globally available
    if let Ok(array_wrapper) = scope.globals.get_item("_ArrayWrapper", vm) {
        vm.builtins.set_attr("_ArrayWrapper", array_wrapper, vm).ok();
    }
    if let Ok(array_result) = scope.globals.get_item("_ArrayResult", vm) {
        vm.builtins.set_attr("_ArrayResult", array_result, vm).ok();
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

