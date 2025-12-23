use rustpython_vm::{PyResult, VirtualMachine, function::FuncArgs};
use crate::tensor::conv::{ConvBackend, ConvParams};

/// xos.ops.convolve(image, kernel, padding="same")
/// Fast 2D convolution operation using tensor backend
/// 
/// - image: frame.array (modified in-place)
/// - kernel: 3D array [height, width, channels] - typically 3x3x3 for RGB
/// - padding: "same" (default) maintains image dimensions
pub fn convolve(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    
    if args_vec.len() < 2 {
        return Err(vm.new_type_error("convolve() requires at least 2 arguments (image, kernel)".to_string()));
    }
    
    let _image_dict = &args_vec[0]; // Image dict (we access frame buffer directly)
    let kernel_arg = &args_vec[1];
    
    // Parse kernel as list of floats
    let kernel_list = kernel_arg.downcast_ref::<rustpython_vm::builtins::PyList>()
        .ok_or_else(|| vm.new_type_error("kernel must be a list".to_string()))?;
    
    let kernel_vec = kernel_list.borrow_vec();
    
    // Kernel should be flattened 3x3x3 = 27 elements
    if kernel_vec.len() != 27 {
        return Err(vm.new_value_error(format!(
            "kernel must have 27 elements (3x3x3), got {}",
            kernel_vec.len()
        )));
    }
    
    // Parse kernel values
    let mut kernel: [f32; 27] = [0.0; 27];
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
    // For RGB conv: kernel is [3x3x3] where each RGB output depends on all RGB inputs
    // Output format: [3, 3, 3, 3] = [out_c=3, in_c=3, kh=3, kw=3]
    let mut kernel_nchw = vec![0.0f32; 3 * 3 * 3 * 3];
    for out_c in 0..3 {
        for in_c in 0..3 {
            for ky in 0..3 {
                for kx in 0..3 {
                    // Old format: [ky, kx, channel_triplet] = [(ky*3 + kx)*3 + channel]
                    let src_idx = (ky * 3 + kx) * 3 + in_c;
                    // New format: [out_c, in_c, ky, kx]
                    let dst_idx = ((out_c * 3 + in_c) * 3 + ky) * 3 + kx;
                    kernel_nchw[dst_idx] = kernel[src_idx];
                }
            }
        }
    }
    
    // Set up convolution parameters for "same" padding
    // For "same" padding with stride=1 and kernel=3x3, we need pad=1
    let params = ConvParams {
        batch: 1,
        in_channels: 3,
        out_channels: 3,
        in_h: height as u32,
        in_w: width as u32,
        kernel_h: 3,
        kernel_w: 3,
        stride_h: 1,
        stride_w: 1,
        pad_h: 1,
        pad_w: 1,
        out_h: height as u32,
        out_w: width as u32,
    };
    
    // Allocate output buffer
    let mut output_nchw = vec![0.0f32; width * height * 3];
    
    // Call tensor backend (uses CPU backend)
    // Note: We're working with CPU slices here, not Metal buffers
    let backend: &dyn ConvBackend = &crate::tensor::conv::cpu::CpuBackend::new();
    backend.conv2d(&input_nchw, &kernel_nchw, &mut output_nchw, params);
    
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
    // Each channel gets its own 3x3 kernel
    let mut kernel_depthwise = vec![0.0f32; 3 * 3 * 3];
    for c in 0..3 {
        for ky in 0..3 {
            for kx in 0..3 {
                let src_idx = ky * 3 + kx;
                let dst_idx = (c * 3 + ky) * 3 + kx;
                kernel_depthwise[dst_idx] = kernel[src_idx];
            }
        }
    }
    
    // Set up depthwise convolution parameters
    let params = ConvParams {
        batch: 1,
        in_channels: 3,
        out_channels: 3,  // Same as input for depthwise
        in_h: height as u32,
        in_w: width as u32,
        kernel_h: 3,
        kernel_w: 3,
        stride_h: 1,
        stride_w: 1,
        pad_h: 1,
        pad_w: 1,
        out_h: height as u32,
        out_w: width as u32,
    };
    
    // Allocate output buffer
    let mut output_nchw = vec![0.0f32; width * height * 3];
    
    // Call tensor backend
    let backend: &dyn ConvBackend = &crate::tensor::conv::cpu::CpuBackend::new();
    backend.depthwise_conv2d(&input_nchw, &kernel_depthwise, &mut output_nchw, params);
    
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

