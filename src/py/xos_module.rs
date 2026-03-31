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

/// xos.frame.clear(...) - clear current frame buffer context
/// Supports:
/// - clear()
/// - clear((r, g, b)) or clear((r, g, b, a))
/// - clear(r, g, b) or clear(r, g, b, a)
fn frame_clear(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;

    let parse_rgba_from_tuple = |tuple_obj: &rustpython_vm::builtins::PyTuple| -> PyResult<(i32, i32, i32, i32)> {
        let items = tuple_obj.as_slice();
        if items.len() == 3 {
            let r: i32 = items[0].clone().try_into_value(vm)?;
            let g: i32 = items[1].clone().try_into_value(vm)?;
            let b: i32 = items[2].clone().try_into_value(vm)?;
            Ok((r, g, b, 255))
        } else if items.len() == 4 {
            let r: i32 = items[0].clone().try_into_value(vm)?;
            let g: i32 = items[1].clone().try_into_value(vm)?;
            let b: i32 = items[2].clone().try_into_value(vm)?;
            let a: i32 = items[3].clone().try_into_value(vm)?;
            Ok((r, g, b, a))
        } else {
            Err(vm.new_type_error("color tuple must be (r, g, b) or (r, g, b, a)".to_string()))
        }
    };

    let (r, g, b, a): (i32, i32, i32, i32) = match args_vec.len() {
        0 => (0, 0, 0, 255),
        1 => {
            let color_tuple = args_vec[0]
                .downcast_ref::<rustpython_vm::builtins::PyTuple>()
                .ok_or_else(|| vm.new_type_error("clear(color): color must be a tuple".to_string()))?;
            parse_rgba_from_tuple(color_tuple)?
        }
        3 => {
            let r: i32 = args_vec[0].clone().try_into_value(vm)?;
            let g: i32 = args_vec[1].clone().try_into_value(vm)?;
            let b: i32 = args_vec[2].clone().try_into_value(vm)?;
            (r, g, b, 255)
        }
        4 => {
            let r: i32 = args_vec[0].clone().try_into_value(vm)?;
            let g: i32 = args_vec[1].clone().try_into_value(vm)?;
            let b: i32 = args_vec[2].clone().try_into_value(vm)?;
            let a: i32 = args_vec[3].clone().try_into_value(vm)?;
            (r, g, b, a)
        }
        _ => {
            return Err(vm.new_type_error(
                "clear() accepts (), (r,g,b), (r,g,b,a), r,g,b, or r,g,b,a".to_string(),
            ));
        }
    };

    let buffer_ptr_opt = crate::python_api::rasterizer::CURRENT_FRAME_BUFFER
        .lock()
        .unwrap()
        .as_ref()
        .map(|ptr| ptr.0);
    let width = *crate::python_api::rasterizer::CURRENT_FRAME_WIDTH.lock().unwrap();
    let height = *crate::python_api::rasterizer::CURRENT_FRAME_HEIGHT.lock().unwrap();

    let buffer_ptr = buffer_ptr_opt.ok_or_else(|| {
        vm.new_runtime_error("No frame buffer context set. frame.clear must be called during tick().".to_string())
    })?;

    let buffer_len = width * height * 4;
    let buffer = unsafe { std::slice::from_raw_parts_mut(buffer_ptr, buffer_len) };
    crate::python_api::rasterizer::fill_buffer_solid_rgba(
        buffer,
        r.clamp(0, 255) as u8,
        g.clamp(0, 255) as u8,
        b.clamp(0, 255) as u8,
        a.clamp(0, 255) as u8,
    );

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
    let random_module = crate::python_api::random::random::make_random_module(vm);
    module.set_attr("random", random_module, vm).unwrap();

    // Add a lightweight string constants submodule for Python compatibility.
    let string_module = vm.new_module("xos.string", vm.ctx.new_dict(), None);
    string_module
        .set_attr(
            "ascii_letters",
            vm.ctx.new_str("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"),
            vm,
        )
        .unwrap();
    string_module
        .set_attr("digits", vm.ctx.new_str("0123456789"), vm)
        .unwrap();
    module.set_attr("string", string_module, vm).unwrap();
    
    // Add the rasterizer submodule
    let rasterizer_module = crate::python_api::rasterizer::make_rasterizer_module(vm);
    module.set_attr("rasterizer", rasterizer_module, vm).unwrap();

    // Add the frame submodule
    let frame_module = vm.new_module("xos.frame", vm.ctx.new_dict(), None);
    frame_module
        .set_attr("clear", vm.new_function("clear", frame_clear), vm)
        .unwrap();
    module.set_attr("frame", frame_module, vm).unwrap();

    // Add color palette submodule (Minecraft 16 dye colors, RGB tuples)
    let color_module = vm.new_module("xos.color", vm.ctx.new_dict(), None);
    color_module.set_attr("white", vm.ctx.new_tuple(vec![vm.ctx.new_int(249).into(), vm.ctx.new_int(255).into(), vm.ctx.new_int(254).into()]), vm).unwrap();
    color_module.set_attr("orange", vm.ctx.new_tuple(vec![vm.ctx.new_int(249).into(), vm.ctx.new_int(128).into(), vm.ctx.new_int(29).into()]), vm).unwrap();
    color_module.set_attr("magenta", vm.ctx.new_tuple(vec![vm.ctx.new_int(199).into(), vm.ctx.new_int(78).into(), vm.ctx.new_int(189).into()]), vm).unwrap();
    color_module.set_attr("light_blue", vm.ctx.new_tuple(vec![vm.ctx.new_int(58).into(), vm.ctx.new_int(179).into(), vm.ctx.new_int(218).into()]), vm).unwrap();
    color_module.set_attr("yellow", vm.ctx.new_tuple(vec![vm.ctx.new_int(254).into(), vm.ctx.new_int(216).into(), vm.ctx.new_int(61).into()]), vm).unwrap();
    color_module.set_attr("lime", vm.ctx.new_tuple(vec![vm.ctx.new_int(128).into(), vm.ctx.new_int(199).into(), vm.ctx.new_int(31).into()]), vm).unwrap();
    color_module.set_attr("pink", vm.ctx.new_tuple(vec![vm.ctx.new_int(243).into(), vm.ctx.new_int(139).into(), vm.ctx.new_int(170).into()]), vm).unwrap();
    color_module.set_attr("gray", vm.ctx.new_tuple(vec![vm.ctx.new_int(71).into(), vm.ctx.new_int(79).into(), vm.ctx.new_int(82).into()]), vm).unwrap();
    color_module.set_attr("light_gray", vm.ctx.new_tuple(vec![vm.ctx.new_int(157).into(), vm.ctx.new_int(157).into(), vm.ctx.new_int(151).into()]), vm).unwrap();
    color_module.set_attr("cyan", vm.ctx.new_tuple(vec![vm.ctx.new_int(22).into(), vm.ctx.new_int(156).into(), vm.ctx.new_int(156).into()]), vm).unwrap();
    color_module.set_attr("purple", vm.ctx.new_tuple(vec![vm.ctx.new_int(137).into(), vm.ctx.new_int(50).into(), vm.ctx.new_int(184).into()]), vm).unwrap();
    color_module.set_attr("blue", vm.ctx.new_tuple(vec![vm.ctx.new_int(60).into(), vm.ctx.new_int(68).into(), vm.ctx.new_int(170).into()]), vm).unwrap();
    color_module.set_attr("brown", vm.ctx.new_tuple(vec![vm.ctx.new_int(131).into(), vm.ctx.new_int(84).into(), vm.ctx.new_int(50).into()]), vm).unwrap();
    color_module.set_attr("green", vm.ctx.new_tuple(vec![vm.ctx.new_int(94).into(), vm.ctx.new_int(124).into(), vm.ctx.new_int(22).into()]), vm).unwrap();
    color_module.set_attr("red", vm.ctx.new_tuple(vec![vm.ctx.new_int(176).into(), vm.ctx.new_int(46).into(), vm.ctx.new_int(38).into()]), vm).unwrap();
    color_module.set_attr("black", vm.ctx.new_tuple(vec![vm.ctx.new_int(29).into(), vm.ctx.new_int(29).into(), vm.ctx.new_int(33).into()]), vm).unwrap();

    // Uppercase aliases for ergonomics (e.g. xos.color.BLACK)
    if let Ok(v) = color_module.get_attr("white", vm) { color_module.set_attr("WHITE", v, vm).unwrap(); }
    if let Ok(v) = color_module.get_attr("orange", vm) { color_module.set_attr("ORANGE", v, vm).unwrap(); }
    if let Ok(v) = color_module.get_attr("magenta", vm) { color_module.set_attr("MAGENTA", v, vm).unwrap(); }
    if let Ok(v) = color_module.get_attr("light_blue", vm) { color_module.set_attr("LIGHT_BLUE", v, vm).unwrap(); }
    if let Ok(v) = color_module.get_attr("yellow", vm) { color_module.set_attr("YELLOW", v, vm).unwrap(); }
    if let Ok(v) = color_module.get_attr("lime", vm) { color_module.set_attr("LIME", v, vm).unwrap(); }
    if let Ok(v) = color_module.get_attr("pink", vm) { color_module.set_attr("PINK", v, vm).unwrap(); }
    if let Ok(v) = color_module.get_attr("gray", vm) { color_module.set_attr("GRAY", v, vm).unwrap(); }
    if let Ok(v) = color_module.get_attr("light_gray", vm) { color_module.set_attr("LIGHT_GRAY", v, vm).unwrap(); }
    if let Ok(v) = color_module.get_attr("cyan", vm) { color_module.set_attr("CYAN", v, vm).unwrap(); }
    if let Ok(v) = color_module.get_attr("purple", vm) { color_module.set_attr("PURPLE", v, vm).unwrap(); }
    if let Ok(v) = color_module.get_attr("blue", vm) { color_module.set_attr("BLUE", v, vm).unwrap(); }
    if let Ok(v) = color_module.get_attr("brown", vm) { color_module.set_attr("BROWN", v, vm).unwrap(); }
    if let Ok(v) = color_module.get_attr("green", vm) { color_module.set_attr("GREEN", v, vm).unwrap(); }
    if let Ok(v) = color_module.get_attr("red", vm) { color_module.set_attr("RED", v, vm).unwrap(); }
    if let Ok(v) = color_module.get_attr("black", vm) { color_module.set_attr("BLACK", v, vm).unwrap(); }
    module.set_attr("color", color_module, vm).unwrap();
    
    // Add the sensors submodule
    let sensors_module = crate::python_api::sensors::make_sensors_module(vm);
    module.set_attr("sensors", sensors_module, vm).unwrap();
    
    // Add the audio submodule
    let audio_module = crate::python_api::audio::make_audio_module(vm);
    module.set_attr("audio", audio_module, vm).unwrap();
    
    // Add the system submodule
    let system_module = crate::python_api::system::make_system_module(vm);
    module.set_attr("system", system_module, vm).unwrap();
    
    // Add the dialoguer submodule
    let dialoguer_module = crate::python_api::dialoguer::make_dialoguer_module(vm);
    module.set_attr("dialoguer", dialoguer_module, vm).unwrap();
    
    // Add the math submodule
    let math_module = crate::python_api::math::make_math_module(vm);
    module.set_attr("math", math_module, vm).unwrap();
    
    // Add the ops submodule
    let ops_module = crate::python_api::ops::make_ops_module(vm);
    module.set_attr("ops", ops_module, vm).unwrap();
    
    // Add the tensors submodule (Burn-backed, replaces array)
    let tensors_module = crate::python_api::tensors::make_tensors_module(vm);
    module.set_attr("tensor", tensors_module.get_attr("tensor", vm).unwrap(), vm).unwrap();
    module.set_attr("zeros", tensors_module.get_attr("zeros", vm).unwrap(), vm).unwrap();
    module.set_attr("ones", tensors_module.get_attr("ones", vm).unwrap(), vm).unwrap();
    module.set_attr("full", tensors_module.get_attr("full", vm).unwrap(), vm).unwrap();
    module.set_attr("arange", tensors_module.get_attr("arange", vm).unwrap(), vm).unwrap();
    module.set_attr("stack", tensors_module.get_attr("stack", vm).unwrap(), vm).unwrap();
    
    // Add the data submodule
    let data_module = crate::python_api::data::make_data_module(vm);
    module.set_attr("data", data_module, vm).unwrap();
    
    // Add the ui submodule
    let ui_module = crate::python_api::ui::make_ui_module(vm);
    module.set_attr("ui", ui_module, vm).unwrap();
    
    // Add the dtypes module and expose dtype constants
    let dtypes_module = crate::python_api::dtypes::make_dtypes_module(vm);
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
    let application_class_code = crate::python_api::engine::pyapp::APPLICATION_CLASS_CODE;
    
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

