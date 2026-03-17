use rustpython_vm::{PyResult, VirtualMachine, function::FuncArgs};
use crate::tensor::conv::{conv2d, depthwise_conv2d};

/// xos.ops.convolve(image, kernel, padding="same")
/// Fast 2D convolution operation using tensor backend
/// 
/// - image: frame.array (modified in-place)
/// - kernel: 3D array [height, width, channels] - e.g., KxKx3 for RGB
/// - padding: "same" (default) maintains image dimensions
/// 
/// Note: Automatically detects kernel size from array length
pub fn convolve(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    
    if args_vec.len() < 2 {
        return Err(vm.new_type_error("convolve() requires at least 2 arguments (image, kernel)".to_string()));
    }
    
    let _image_dict = &args_vec[0]; // Image dict (we access frame buffer directly)
    let kernel_arg = &args_vec[1];
    
    // Try to extract _data if kernel is an _ArrayWrapper or _ArrayResult
    let kernel_list_obj = if let Ok(data_attr) = kernel_arg.get_attr("_data", vm) {
        data_attr
    } else {
        kernel_arg.clone()
    };
    
    // Parse kernel as list of floats
    let kernel_list = kernel_list_obj.downcast_ref::<rustpython_vm::builtins::PyList>()
        .ok_or_else(|| vm.new_type_error("kernel must be a list or array".to_string()))?;
    
    let kernel_vec = kernel_list.borrow_vec();
    
    // Infer kernel size: for RGB conv, length should be K*K*3
    // where K is the spatial kernel size
    let kernel_len = kernel_vec.len();
    if kernel_len % 3 != 0 {
        return Err(vm.new_value_error(format!(
            "kernel length must be divisible by 3 (for RGB), got {}",
            kernel_len
        )));
    }
    
    let spatial_len = kernel_len / 3;
    let kernel_size = (spatial_len as f32).sqrt() as usize;
    
    if kernel_size * kernel_size * 3 != kernel_len {
        return Err(vm.new_value_error(format!(
            "kernel must be square (KxKx3), got {} elements",
            kernel_len
        )));
    }
    
    // Parse kernel values
    let mut kernel: Vec<f32> = Vec::with_capacity(kernel_len);
    for val in kernel_vec.iter() {
        let f: f64 = val.clone().try_into_value(vm)?;
        kernel.push(f as f32);
    }
    
    drop(kernel_vec);
    
    // Get the frame buffer from global context
    let buffer_ptr_opt = crate::python::rasterizer::CURRENT_FRAME_BUFFER.lock().unwrap().as_ref().map(|ptr| ptr.0);
    let width = *crate::python::rasterizer::CURRENT_FRAME_WIDTH.lock().unwrap();
    let height = *crate::python::rasterizer::CURRENT_FRAME_HEIGHT.lock().unwrap();
    
    let buffer_ptr = buffer_ptr_opt.ok_or_else(|| {
        vm.new_runtime_error("No frame buffer context set. convolve must be called during tick().".to_string())
    })?;
    
    let buffer_len = width * height * 4;
    let buffer = unsafe { std::slice::from_raw_parts_mut(buffer_ptr, buffer_len) };
    
    // Convert u8 RGBA buffer to f32 RGB channels (batch=1, channels=3)
    let mut input_f32 = vec![0.0f32; width * height * 3];
    for i in 0..(width * height) {
        let src_idx = i * 4;
        let dst_idx = i * 3;
        input_f32[dst_idx + 0] = buffer[src_idx + 0] as f32;
        input_f32[dst_idx + 1] = buffer[src_idx + 1] as f32;
        input_f32[dst_idx + 2] = buffer[src_idx + 2] as f32;
    }
    
    // Reorganize input to batch-channel-height-width format (NCHW)
    // From [height*width*3] to [1, 3, height, width]
    let mut input_nchw = vec![0.0f32; width * height * 3];
    for y in 0..height {
        for x in 0..width {
            let src_idx = (y * width + x) * 3;
            for c in 0..3 {
                let dst_idx = (c * height + y) * width + x;
                input_nchw[dst_idx] = input_f32[src_idx + c];
            }
        }
    }
    
    // Reorganize kernel from [ky, kx, channel] to [out_channel, in_channel, ky, kx]
    // For RGB conv: kernel is [KxKx3] where each RGB output depends on all RGB inputs
    // Output format: [3, 3, K, K] = [out_c=3, in_c=3, kh=K, kw=K]
    let mut kernel_nchw = vec![0.0f32; 3 * 3 * kernel_size * kernel_size];
    for out_c in 0..3 {
        for in_c in 0..3 {
            for ky in 0..kernel_size {
                for kx in 0..kernel_size {
                    // Old format: [ky, kx, channel_triplet] = [(ky*K + kx)*3 + channel]
                    let src_idx = (ky * kernel_size + kx) * 3 + in_c;
                    // New format: [out_c, in_c, ky, kx]
                    let dst_idx = ((out_c * 3 + in_c) * kernel_size + ky) * kernel_size + kx;
                    kernel_nchw[dst_idx] = kernel[src_idx];
                }
            }
        }
    }
    
    // Set up convolution parameters for "same" padding
    let pad = (kernel_size - 1) / 2;
    
    // Allocate output buffer
    let mut output_nchw = vec![0.0f32; width * height * 3];
    
    conv2d(
        &input_nchw,
        &kernel_nchw,
        &mut output_nchw,
        1,      // batch
        3,      // in_channels
        3,      // out_channels
        height as usize,
        width as usize,
        kernel_size,
        kernel_size,
        [1, 1], // stride
        [pad, pad], // padding
    );
    
    // Convert back from NCHW to interleaved RGB
    let mut output_rgb = vec![0u8; buffer_len];
    for y in 0..height {
        for x in 0..width {
            let dst_idx = (y * width + x) * 4;
            for c in 0..3 {
                let src_idx = (c * height + y) * width + x;
                output_rgb[dst_idx + c] = output_nchw[src_idx].clamp(0.0, 255.0) as u8;
            }
            output_rgb[dst_idx + 3] = 255; // Alpha
        }
    }
    
    // Return output as Python list wrapped in _ArrayResult
    let py_list: Vec<rustpython_vm::PyObjectRef> = output_rgb.iter()
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

/// xos.ops.convolve_depthwise(image, kernel, padding="same")
/// Fast 2D depthwise convolution - each channel processed independently using tensor backend
/// 
/// - image: frame.array (modified in-place)
/// - kernel: 2D array [height, width] = KxK values (applied to each channel separately)
/// - padding: "same" (default) maintains image dimensions
/// 
/// Note: Automatically detects kernel size from array length
pub fn convolve_depthwise(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    
    if args_vec.len() < 2 {
        return Err(vm.new_type_error("convolve_depthwise() requires at least 2 arguments (image, kernel)".to_string()));
    }
    
    let _image_dict = &args_vec[0]; // Image dict (we access frame buffer directly)
    let kernel_arg = &args_vec[1];
    
    // Try to extract _data if kernel is an _ArrayWrapper or _ArrayResult
    let kernel_list_obj = if let Ok(data_attr) = kernel_arg.get_attr("_data", vm) {
        data_attr
    } else {
        kernel_arg.clone()
    };
    
    // Parse kernel as list of floats
    let kernel_list = kernel_list_obj.downcast_ref::<rustpython_vm::builtins::PyList>()
        .ok_or_else(|| vm.new_type_error("kernel must be a list or array".to_string()))?;
    
    let kernel_vec = kernel_list.borrow_vec();
    
    // Infer kernel size: for depthwise conv, length should be K*K
    let kernel_len = kernel_vec.len();
    let kernel_size = (kernel_len as f32).sqrt() as usize;
    
    if kernel_size * kernel_size != kernel_len {
        return Err(vm.new_value_error(format!(
            "kernel must be square (KxK), got {} elements",
            kernel_len
        )));
    }
    
    // Parse kernel values
    let mut kernel: Vec<f32> = Vec::with_capacity(kernel_len);
    for val in kernel_vec.iter() {
        let f: f64 = val.clone().try_into_value(vm)?;
        kernel.push(f as f32);
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
    
    // Convert u8 RGBA buffer to f32 RGB channels
    let mut input_f32 = vec![0.0f32; width * height * 3];
    for i in 0..(width * height) {
        let src_idx = i * 4;
        let dst_idx = i * 3;
        input_f32[dst_idx + 0] = buffer[src_idx + 0] as f32;
        input_f32[dst_idx + 1] = buffer[src_idx + 1] as f32;
        input_f32[dst_idx + 2] = buffer[src_idx + 2] as f32;
    }
    
    // Reorganize input to NCHW format
    let mut input_nchw = vec![0.0f32; width * height * 3];
    for y in 0..height {
        for x in 0..width {
            let src_idx = (y * width + x) * 3;
            for c in 0..3 {
                let dst_idx = (c * height + y) * width + x;
                input_nchw[dst_idx] = input_f32[src_idx + c];
            }
        }
    }
    
    // Organize kernel for depthwise conv: [channel, ky, kx]
    // Each channel gets its own KxK kernel
    let mut kernel_depthwise = vec![0.0f32; 3 * kernel_size * kernel_size];
    for c in 0..3 {
        for ky in 0..kernel_size {
            for kx in 0..kernel_size {
                let src_idx = ky * kernel_size + kx;
                let dst_idx = (c * kernel_size + ky) * kernel_size + kx;
                kernel_depthwise[dst_idx] = kernel[src_idx];
            }
        }
    }
    
    let pad = (kernel_size - 1) / 2;
    
    // Allocate output buffer
    let mut output_nchw = vec![0.0f32; width * height * 3];
    
    depthwise_conv2d(
        &input_nchw,
        &kernel_depthwise,
        &mut output_nchw,
        1,      // batch
        3,      // channels
        height as usize,
        width as usize,
        kernel_size,
        kernel_size,
        [1, 1], // stride
        [pad, pad], // padding
    );
    
    // Convert back from NCHW to interleaved RGBA
    let mut output_rgb = vec![0u8; buffer_len];
    for y in 0..height {
        for x in 0..width {
            let dst_idx = (y * width + x) * 4;
            for c in 0..3 {
                let src_idx = (c * height + y) * width + x;
                output_rgb[dst_idx + c] = output_nchw[src_idx].clamp(0.0, 255.0) as u8;
            }
            output_rgb[dst_idx + 3] = 255; // Alpha
        }
    }
    
    // Return output as Python list wrapped in _ArrayResult
    let py_list: Vec<rustpython_vm::PyObjectRef> = output_rgb.iter()
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

