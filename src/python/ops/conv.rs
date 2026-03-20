use rustpython_vm::{PyObjectRef, PyResult, VirtualMachine, function::FuncArgs};
use crate::tensor::conv::{conv2d, depthwise_conv2d};

/// Extract the underlying data list from an array/tensor (handles _ArrayWrapper, dict, or list)
fn get_array_data_list(obj: &PyObjectRef, vm: &VirtualMachine) -> PyResult<Option<PyObjectRef>> {
    // _ArrayWrapper: get_attr("_data") returns the inner dict; dict["_data"] is the list
    if let Ok(data_attr) = obj.get_attr("_data", vm) {
        if let Ok(inner_dict) = data_attr.clone().downcast::<rustpython_vm::builtins::PyDict>() {
            if let Ok(list_obj) = inner_dict.get_item("_data", vm) {
                if list_obj.downcast_ref::<rustpython_vm::builtins::PyList>().is_some() {
                    return Ok(Some(list_obj));
                }
            }
        }
        // _data is already the list
        if data_attr.downcast_ref::<rustpython_vm::builtins::PyList>().is_some() {
            return Ok(Some(data_attr));
        }
    }
    // Raw dict
    if let Ok(dict) = obj.clone().downcast::<rustpython_vm::builtins::PyDict>() {
        if let Ok(list_obj) = dict.get_item("_data", vm) {
            if list_obj.downcast_ref::<rustpython_vm::builtins::PyList>().is_some() {
                return Ok(Some(list_obj));
            }
        }
    }
    // Plain list
    if obj.downcast_ref::<rustpython_vm::builtins::PyList>().is_some() {
        return Ok(Some(obj.clone()));
    }
    Ok(None)
}

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
    
    let kernel_list = get_array_data_list(kernel_arg, vm)?
        .and_then(|o| o.downcast::<rustpython_vm::builtins::PyList>().ok())
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
    // Normalize by L1 norm so output stays in ~[-255,255] for u8 input - prevents rapid blackout
    let norm: f32 = kernel.iter().map(|&x| x.abs()).sum::<f32>().max(1e-6);
    kernel.iter_mut().for_each(|x| *x /= norm);
    
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
    // Normalized kernel gives output ~[-255,255]; map to u8: (out+255)/2, then clamp
    let mut output_rgb = vec![0u8; buffer_len];
    for y in 0..height {
        for x in 0..width {
            let dst_idx = (y * width + x) * 4;
            for c in 0..3 {
                let src_idx = (c * height + y) * width + x;
                let v = (output_nchw[src_idx] + 255.0) / 2.0;
                output_rgb[dst_idx + c] = v.clamp(0.0, 255.0) as u8;
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

/// Extract shape [H, W, C] from array/tensor
fn get_image_shape(obj: &PyObjectRef, vm: &VirtualMachine) -> PyResult<Option<(usize, usize, usize)>> {
    let shape_obj = if let Some(dict) = obj.downcast_ref::<rustpython_vm::builtins::PyDict>() {
        dict.get_item("shape", vm).ok()
    } else {
        obj.get_attr("shape", vm).ok()
    };
    let shape_obj = match shape_obj {
        Some(s) => s,
        None => return Ok(None),
    };
    let s: &[PyObjectRef] = match shape_obj.downcast_ref::<rustpython_vm::builtins::PyTuple>() {
        Some(t) => t.as_slice(),
        None => return Ok(None),
    };
    if s.len() >= 3 {
        let h: i64 = s[0].clone().try_into_value(vm).ok().ok_or_else(|| vm.new_type_error("shape H must be int".to_string()))?;
        let w: i64 = s[1].clone().try_into_value(vm).ok().ok_or_else(|| vm.new_type_error("shape W must be int".to_string()))?;
        let c: i64 = s[2].clone().try_into_value(vm).unwrap_or(3);
        Ok(Some((h as usize, w as usize, c.max(1) as usize)))
    } else if s.len() == 2 {
        let h: i64 = s[0].clone().try_into_value(vm).ok().ok_or_else(|| vm.new_type_error("shape H must be int".to_string()))?;
        let w: i64 = s[1].clone().try_into_value(vm).ok().ok_or_else(|| vm.new_type_error("shape W must be int".to_string()))?;
        Ok(Some((h as usize, w as usize, 3)))
    } else {
        Ok(None)
    }
}

/// xos.ops.convolve_image(image, kernel) - convolve arbitrary image tensor, returns new image dict
/// - image: tensor/array with _data and shape (H, W, 3). Values 0-255 (int or float).
/// - kernel: 3x3x3. Returns dict with _data (flat u8 RGBA) and shape.
pub fn convolve_image(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() < 2 {
        return Err(vm.new_type_error("convolve_image() requires (image, kernel)".to_string()));
    }
    let image_arg = &args_vec[0];
    let kernel_arg = &args_vec[1];

    let (height, width, _channels) = get_image_shape(image_arg, vm)
        .ok()
        .flatten()
        .ok_or_else(|| vm.new_type_error("image must have shape (H, W, 3)".to_string()))?;

    let data_list = get_array_data_list(image_arg, vm)?
        .and_then(|o| o.downcast::<rustpython_vm::builtins::PyList>().ok())
        .ok_or_else(|| vm.new_type_error("image must have _data list".to_string()))?;
    let data_vec = data_list.borrow_vec();

    // Parse image to f32 (int or float 0-255)
    let mut input_f32 = Vec::with_capacity(width * height * 3);
    for item in data_vec.iter() {
        let v: f64 = item.clone().try_into_value(vm).or_else(|_| item.clone().try_into_value::<i64>(vm).map(|x| x as f64))?;
        input_f32.push(v.clamp(0.0, 255.0) as f32);
    }
    drop(data_vec);

    // Kernel parsing (same as convolve)
    let kernel_list = get_array_data_list(kernel_arg, vm)?
        .and_then(|o| o.downcast::<rustpython_vm::builtins::PyList>().ok())
        .ok_or_else(|| vm.new_type_error("kernel must be a list or array".to_string()))?;
    let kernel_vec = kernel_list.borrow_vec();
    let kernel_len = kernel_vec.len();
    if kernel_len % 3 != 0 {
        return Err(vm.new_value_error("kernel must be KxKx3".to_string()));
    }
    let kernel_size = (kernel_len / 3) as f32;
    let kernel_size = kernel_size.sqrt() as usize;
    let mut kernel: Vec<f32> = Vec::with_capacity(kernel_len);
    for val in kernel_vec.iter() {
        let f: f64 = val.clone().try_into_value(vm)?;
        kernel.push(f as f32);
    }
    let norm: f32 = kernel.iter().map(|&x| x.abs()).sum::<f32>().max(1e-6);
    kernel.iter_mut().for_each(|x| *x /= norm);
    drop(kernel_vec);

    // HWC -> NCHW
    let mut input_nchw = vec![0.0f32; width * height * 3];
    for y in 0..height {
        for x in 0..width {
            for c in 0..3 {
                let src_idx = (y * width + x) * 3 + c;
                let dst_idx = (c * height + y) * width + x;
                input_nchw[dst_idx] = input_f32[src_idx];
            }
        }
    }

    let mut kernel_nchw = vec![0.0f32; 3 * 3 * kernel_size * kernel_size];
    for out_c in 0..3 {
        for in_c in 0..3 {
            for ky in 0..kernel_size {
                for kx in 0..kernel_size {
                    let src_idx = (ky * kernel_size + kx) * 3 + in_c;
                    let dst_idx = ((out_c * 3 + in_c) * kernel_size + ky) * kernel_size + kx;
                    kernel_nchw[dst_idx] = kernel[src_idx];
                }
            }
        }
    }

    let pad = (kernel_size - 1) / 2;
    let mut output_nchw = vec![0.0f32; width * height * 3];
    conv2d(
        &input_nchw,
        &kernel_nchw,
        &mut output_nchw,
        1, 3, 3, height, width, kernel_size, kernel_size, [1, 1], [pad, pad],
    );

    let mut output_rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            for c in 0..3 {
                let src_idx = (c * height + y) * width + x;
                let v = (output_nchw[src_idx] + 255.0) / 2.0;
                output_rgba.push(v.clamp(0.0, 255.0) as u8);
            }
            output_rgba.push(255);
        }
    }

    let dict = vm.ctx.new_dict();
    let py_list: Vec<_> = output_rgba.iter().map(|&b| vm.ctx.new_int(b).into()).collect();
    dict.set_item("_data", vm.ctx.new_list(py_list).into(), vm)?;
    dict.set_item("shape", vm.ctx.new_tuple(vec![
        vm.ctx.new_int(height).into(),
        vm.ctx.new_int(width).into(),
        vm.ctx.new_int(4).into(),
    ]).into(), vm)?;
    Ok(dict.into())
}

/// xos.ops.convolve_depthwise_image(image, kernel) - same but depthwise
pub fn convolve_depthwise_image(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() < 2 {
        return Err(vm.new_type_error("convolve_depthwise_image() requires (image, kernel)".to_string()));
    }
    let image_arg = &args_vec[0];
    let kernel_arg = &args_vec[1];

    let (height, width, _) = get_image_shape(image_arg, vm)
        .ok()
        .flatten()
        .ok_or_else(|| vm.new_type_error("image must have shape (H, W, 3)".to_string()))?;

    let data_list = get_array_data_list(image_arg, vm)?
        .and_then(|o| o.downcast::<rustpython_vm::builtins::PyList>().ok())
        .ok_or_else(|| vm.new_type_error("image must have _data list".to_string()))?;
    let data_vec = data_list.borrow_vec();
    let mut input_f32 = Vec::with_capacity(width * height * 3);
    for item in data_vec.iter() {
        let v: f64 = item.clone().try_into_value(vm).or_else(|_| item.clone().try_into_value::<i64>(vm).map(|x| x as f64))?;
        input_f32.push(v.clamp(0.0, 255.0) as f32);
    }
    drop(data_vec);

    let kernel_list = get_array_data_list(kernel_arg, vm)?
        .and_then(|o| o.downcast::<rustpython_vm::builtins::PyList>().ok())
        .ok_or_else(|| vm.new_type_error("kernel must be a list or array".to_string()))?;
    let kernel_vec = kernel_list.borrow_vec();
    let kernel_len = kernel_vec.len();
    let kernel_size = (kernel_len as f32).sqrt() as usize;
    if kernel_size * kernel_size != kernel_len {
        return Err(vm.new_value_error("kernel must be KxK".to_string()));
    }
    let mut kernel: Vec<f32> = Vec::with_capacity(kernel_len);
    for val in kernel_vec.iter() {
        let f: f64 = val.clone().try_into_value(vm)?;
        kernel.push(f as f32);
    }
    let norm: f32 = kernel.iter().map(|&x| x.abs()).sum::<f32>().max(1e-6);
    kernel.iter_mut().for_each(|x| *x /= norm);
    drop(kernel_vec);

    let mut input_nchw = vec![0.0f32; width * height * 3];
    for y in 0..height {
        for x in 0..width {
            for c in 0..3 {
                let src_idx = (y * width + x) * 3 + c;
                let dst_idx = (c * height + y) * width + x;
                input_nchw[dst_idx] = input_f32[src_idx];
            }
        }
    }

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
    let mut output_nchw = vec![0.0f32; width * height * 3];
    depthwise_conv2d(
        &input_nchw,
        &kernel_depthwise,
        &mut output_nchw,
        1, 3, height, width, kernel_size, kernel_size, [1, 1], [pad, pad],
    );

    let mut output_rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            for c in 0..3 {
                let src_idx = (c * height + y) * width + x;
                let v = (output_nchw[src_idx] + 255.0) / 2.0;
                output_rgba.push(v.clamp(0.0, 255.0) as u8);
            }
            output_rgba.push(255);
        }
    }

    let dict = vm.ctx.new_dict();
    let py_list: Vec<_> = output_rgba.iter().map(|&b| vm.ctx.new_int(b).into()).collect();
    dict.set_item("_data", vm.ctx.new_list(py_list).into(), vm)?;
    dict.set_item("shape", vm.ctx.new_tuple(vec![
        vm.ctx.new_int(height).into(),
        vm.ctx.new_int(width).into(),
        vm.ctx.new_int(4).into(),
    ]).into(), vm)?;
    Ok(dict.into())
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
    
    let kernel_list = get_array_data_list(kernel_arg, vm)?
        .and_then(|o| o.downcast::<rustpython_vm::builtins::PyList>().ok())
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
    let norm: f32 = kernel.iter().map(|&x| x.abs()).sum::<f32>().max(1e-6);
    kernel.iter_mut().for_each(|x| *x /= norm);
    
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
    
    // Convert back: normalized output ~[-255,255] -> u8 via (out+255)/2
    let mut output_rgb = vec![0u8; buffer_len];
    for y in 0..height {
        for x in 0..width {
            let dst_idx = (y * width + x) * 4;
            for c in 0..3 {
                let src_idx = (c * height + y) * width + x;
                let v = (output_nchw[src_idx] + 255.0) / 2.0;
                output_rgb[dst_idx + c] = v.clamp(0.0, 255.0) as u8;
            }
            output_rgb[dst_idx + 3] = 255; // Alpha
        }
    }
    
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

