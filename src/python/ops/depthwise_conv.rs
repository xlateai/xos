use rustpython_vm::{PyResult, VirtualMachine, function::FuncArgs};

/// xos.ops.convolve_depthwise(image, kernel, padding="same")
/// Fast 2D depthwise convolution - each channel processed independently
/// 
/// - image: frame.array (modified in-place)
/// - kernel: 2D array [height, width] = 3x3 = 9 values (applied to each channel separately)
/// - padding: "same" (default) maintains image dimensions
pub fn convolve_depthwise(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    
    if args_vec.len() < 2 {
        return Err(vm.new_type_error("convolve_depthwise() requires at least 2 arguments (image, kernel)".to_string()));
    }
    
    let _image_dict = &args_vec[0]; // Image dict (we access frame buffer directly)
    let kernel_arg = &args_vec[1];
    
    // Parse kernel as list of floats
    let kernel_list = kernel_arg.downcast_ref::<rustpython_vm::builtins::PyList>()
        .ok_or_else(|| vm.new_type_error("kernel must be a list".to_string()))?;
    
    let kernel_vec = kernel_list.borrow_vec();
    
    // Kernel should be flattened 3x3 = 9 elements (applied per channel)
    if kernel_vec.len() != 9 {
        return Err(vm.new_value_error(format!(
            "kernel must have 9 elements (3x3 depthwise), got {}",
            kernel_vec.len()
        )));
    }
    
    // Parse kernel values
    let mut kernel: [f32; 9] = [0.0; 9];
    for (i, val) in kernel_vec.iter().enumerate() {
        let f: f64 = val.clone().try_into_value(vm)?;
        kernel[i] = f as f32;
    }
    
    drop(kernel_vec);
    
    // Get the frame buffer from global context
    let buffer_ptr_opt = crate::python::rasterizer::CURRENT_FRAME_BUFFER.lock().unwrap().as_ref().map(|ptr| ptr.0);
    let width = *crate::python::rasterizer::CURRENT_FRAME_WIDTH.lock().unwrap();
    let height = *crate::python::rasterizer::CURRENT_FRAME_HEIGHT.lock().unwrap();
    
    let buffer_ptr = buffer_ptr_opt.ok_or_else(|| {
        vm.new_runtime_error("No frame buffer context set. convolve_depthwise must be called during tick().".to_string())
    })?;
    
    let buffer_len = width * height * 4;
    let buffer = unsafe { std::slice::from_raw_parts_mut(buffer_ptr, buffer_len) };
    
    // Create temporary output buffer
    let mut output = vec![0u8; buffer_len];
    
    // Apply 2D depthwise convolution with "same" padding
    for y in 0..height {
        for x in 0..width {
            // For each pixel, apply 3x3 kernel to each channel independently
            let mut sum_r = 0.0f32;
            let mut sum_g = 0.0f32;
            let mut sum_b = 0.0f32;
            
            // 3x3 kernel centered at (x, y)
            for ky in 0..3 {
                for kx in 0..3 {
                    // Source pixel position (with border handling)
                    let sy = (y as i32 + ky - 1).max(0).min(height as i32 - 1) as usize;
                    let sx = (x as i32 + kx - 1).max(0).min(width as i32 - 1) as usize;
                    
                    let src_idx = (sy * width + sx) * 4;
                    
                    // Same kernel for all channels (depthwise)
                    let k_idx = (ky * 3 + kx) as usize;
                    
                    sum_r += buffer[src_idx + 0] as f32 * kernel[k_idx];
                    sum_g += buffer[src_idx + 1] as f32 * kernel[k_idx];
                    sum_b += buffer[src_idx + 2] as f32 * kernel[k_idx];
                }
            }
            
            // Write to output buffer
            let dst_idx = (y * width + x) * 4;
            output[dst_idx + 0] = sum_r.clamp(0.0, 255.0) as u8;
            output[dst_idx + 1] = sum_g.clamp(0.0, 255.0) as u8;
            output[dst_idx + 2] = sum_b.clamp(0.0, 255.0) as u8;
            output[dst_idx + 3] = 255; // Alpha stays at 255
        }
    }
    
    // Return output as Python list wrapped in _ArrayResult for nice printing
    let py_list: Vec<rustpython_vm::PyObjectRef> = output.iter()
        .map(|&b| vm.ctx.new_int(b).into())
        .collect();
    
    let list_obj = vm.ctx.new_list(py_list);
    
    // Try to wrap in _ArrayResult if available
    if let Ok(wrapper_class) = vm.builtins.get_attr("_ArrayResult", vm) {
        let shape_tuple: rustpython_vm::PyObjectRef = vm.ctx.new_tuple(vec![
            vm.ctx.new_int(height).into(),
            vm.ctx.new_int(width).into(),
            vm.ctx.new_int(4).into(),
        ]).into();
        if let Ok(wrapped) = wrapper_class.call((list_obj.clone(), shape_tuple), vm) {
            return Ok(wrapped);
        }
    }
    
    // Fallback to plain list if wrapper not available
    Ok(list_obj.into())
}

