use rustpython_vm::{PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs};

/// xos.rasterizer.circles() - efficiently draw circles on a frame buffer
/// 
/// Usage: xos.rasterizer.circles(frame, positions, radii, color)
/// - frame: dict with 'width', 'height', 'buffer' (list of RGBA bytes)
/// - positions: list of (x, y) tuples
/// - radii: list of radii (or single radius for all)
/// - color: (r, g, b, a) tuple
fn circles(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    // Extract arguments
    let args_vec = args.args;
    if args_vec.len() != 4 {
        return Err(vm.new_type_error(format!(
            "circles() takes exactly 4 arguments ({} given)",
            args_vec.len()
        )));
    }
    
    let frame_dict = &args_vec[0];
    let positions_list = &args_vec[1];
    let radii_list = &args_vec[2];
    let color_tuple = &args_vec[3];
    
    // frame_dict might be a _FrameWrapper or a plain dict
    // Try to get _data attribute first (if it's a wrapper), otherwise use it directly
    let actual_frame_dict = if let Ok(data_attr) = vm.get_attribute_opt(frame_dict.clone(), "_data") {
        if let Some(data) = data_attr {
            data
        } else {
            frame_dict.clone()
        }
    } else {
        frame_dict.clone()
    };
    
    // actual_frame_dict is a Python dict
    let frame_py_dict = actual_frame_dict.downcast_ref::<rustpython_vm::builtins::PyDict>()
        .ok_or_else(|| vm.new_type_error("frame must be a dict or _FrameWrapper".to_string()))?;
    
    // Extract array from frame
    let array_obj = frame_py_dict.get_item("array", vm)?;
    
    let array_dict = array_obj.downcast_ref::<rustpython_vm::builtins::PyDict>()
        .ok_or_else(|| vm.new_type_error("array must be a dict".to_string()))?;
    
    // Get the buffer (data) from the array FIRST to get accurate size
    let buffer = array_dict.get_item("data", vm)?;
    let buffer_list = buffer.downcast_ref::<rustpython_vm::builtins::PyList>()
        .ok_or_else(|| vm.new_type_error("buffer data must be a list".to_string()))?;
    
    let buffer_len = buffer_list.borrow_vec().len();
    
    // Extract width and height from array shape
    let shape_obj = array_dict.get_item("shape", vm)?;
    let shape_tuple = shape_obj.downcast_ref::<rustpython_vm::builtins::PyTuple>()
        .ok_or_else(|| vm.new_type_error("shape must be a tuple".to_string()))?;
    let shape_vec = shape_tuple.as_slice();
    if shape_vec.len() < 3 {
        return Err(vm.new_type_error("shape must be (height, width, channels)".to_string()));
    }
    
    let height: i32 = shape_vec[0].clone().try_into_value(vm)?;
    let width: i32 = shape_vec[1].clone().try_into_value(vm)?;
    
    // Validate that buffer size matches shape (to catch resize issues)
    let expected_len = (height * width * 4) as usize;
    if buffer_len != expected_len {
        return Err(vm.new_runtime_error(format!(
            "Frame buffer size mismatch: expected {} bytes ({}x{}x4), but buffer has {} bytes. Window may have been resized.",
            expected_len, width, height, buffer_len
        )));
    }
    
    // Parse color tuple (r, g, b, a)
    let color_obj = color_tuple.downcast_ref::<rustpython_vm::builtins::PyTuple>()
        .ok_or_else(|| vm.new_type_error("color must be a tuple".to_string()))?;
    let color_vec = color_obj.as_slice();
    if color_vec.len() != 4 {
        return Err(vm.new_type_error("color must be (r, g, b, a)".to_string()));
    }
    let r: i32 = color_vec[0].clone().try_into_value(vm)?;
    let g: i32 = color_vec[1].clone().try_into_value(vm)?;
    let b: i32 = color_vec[2].clone().try_into_value(vm)?;
    let a: i32 = color_vec[3].clone().try_into_value(vm)?;
    let color = (r as u8, g as u8, b as u8, a as u8);
    
    // Get positions list
    let positions = positions_list.downcast_ref::<rustpython_vm::builtins::PyList>()
        .ok_or_else(|| vm.new_type_error("positions must be a list".to_string()))?;
    
    // Get radii list
    let radii = radii_list.downcast_ref::<rustpython_vm::builtins::PyList>()
        .ok_or_else(|| vm.new_type_error("radii must be a list".to_string()))?;
    
    // Collect all circle data first before drawing (to avoid borrow conflicts)
    let positions_vec = positions.borrow_vec();
    let radii_vec = radii.borrow_vec();
    
    let mut circles_to_draw = Vec::new();
    
    for (i, pos_obj) in positions_vec.iter().enumerate() {
        // Parse position tuple
        let pos_tuple = pos_obj.downcast_ref::<rustpython_vm::builtins::PyTuple>()
            .ok_or_else(|| vm.new_type_error("position must be a tuple".to_string()))?;
        let pos_vec = pos_tuple.as_slice();
        if pos_vec.len() != 2 {
            return Err(vm.new_type_error("position must be (x, y)".to_string()));
        }
        let cx: f64 = pos_vec[0].clone().try_into_value(vm)?;
        let cy: f64 = pos_vec[1].clone().try_into_value(vm)?;
        
        // Get radius (either from list or use first one for all)
        let radius: f64 = if i < radii_vec.len() {
            radii_vec[i].clone().try_into_value(vm)?
        } else if !radii_vec.is_empty() {
            radii_vec[0].clone().try_into_value(vm)?
        } else {
            return Err(vm.new_type_error("radii list is empty".to_string()));
        };
        
        circles_to_draw.push((cx as f32, cy as f32, radius as f32));
    }
    
    // Drop borrows
    drop(positions_vec);
    drop(radii_vec);
    
    // Now draw all circles
    for (cx, cy, radius) in circles_to_draw {
        draw_circle(buffer_list, width as usize, height as usize, cx, cy, radius, color, vm)?;
    }
    
    Ok(vm.ctx.none())
}

fn draw_circle(
    buffer: &rustpython_vm::builtins::PyList,
    width: usize,
    height: usize,
    cx: f32,
    cy: f32,
    radius: f32,
    color: (u8, u8, u8, u8),
    vm: &VirtualMachine,
) -> PyResult<()> {
    let radius_squared = radius * radius;
    
    let start_x = (cx - radius).max(0.0) as usize;
    let end_x = ((cx + radius + 1.0) as usize).min(width);
    let start_y = (cy - radius).max(0.0) as usize;
    let end_y = ((cy + radius + 1.0) as usize).min(height);
    
    let mut buf_vec = buffer.borrow_vec_mut();
    
    for y in start_y..end_y {
        for x in start_x..end_x {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            if dx * dx + dy * dy <= radius_squared {
                let idx = (y * width + x) * 4;
                if idx + 3 < width * height * 4 {
                    // Set RGBA values in the list
                    buf_vec[idx + 0] = vm.ctx.new_int(color.0).into();
                    buf_vec[idx + 1] = vm.ctx.new_int(color.1).into();
                    buf_vec[idx + 2] = vm.ctx.new_int(color.2).into();
                    buf_vec[idx + 3] = vm.ctx.new_int(color.3).into();
                }
            }
        }
    }
    
    Ok(())
}

pub fn make_rasterizer_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.rasterizer", vm.ctx.new_dict(), None);
    module.set_attr("circles", vm.new_function("circles", circles), vm).unwrap();
    module
}
