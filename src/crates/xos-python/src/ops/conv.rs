use xos_tensor::conv::{conv2d, depthwise_conv2d};
use rustpython_vm::{function::FuncArgs, PyObjectRef, PyResult, VirtualMachine};

fn direct_fill_sentinel(vm: &VirtualMachine) -> PyResult {
    let sentinel = vm.ctx.new_dict();
    sentinel.set_item("_direct_fill", vm.ctx.new_bool(true).into(), vm)?;
    Ok(sentinel.into())
}

/// HWC `[ky, kx, in_c]` (K×K×3) → NCHW `[out_c, in_c, kh, kw]`.
fn kernel_hwc_to_nchw(kernel: &[f32], kernel_size: usize) -> Vec<f32> {
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
    kernel_nchw
}

/// When `Application.tick()` is active, convolve on [`FrameState`]'s GPU tensor (no per-frame CPU↔GPU vec path).
#[cfg(not(target_arch = "wasm32"))]
fn try_convolve_on_frame_gpu(
    kernel_nchw: &[f32],
    kernel_size: usize,
    stride: [usize; 2],
) -> bool {
    crate::engine::py_engine_tls::with_tick_engine_state_mut(|state| {
        xos_core::burn_raster::convolve_rgb_same(
            &mut state.frame,
            kernel_nchw.to_vec(),
            kernel_size,
            kernel_size,
            stride,
        )
        .is_ok()
    })
    .unwrap_or(false)
}

#[cfg(not(target_arch = "wasm32"))]
fn try_convolve_depthwise_on_frame_gpu(
    kernel: Vec<f32>,
    kernel_size: usize,
    stride: [usize; 2],
) -> bool {
    crate::engine::py_engine_tls::with_tick_engine_state_mut(|state| {
        xos_core::burn_raster::convolve_depthwise_rgb_same(
            &mut state.frame,
            kernel,
            kernel_size,
            kernel_size,
            stride,
        )
        .is_ok()
    })
    .unwrap_or(false)
}

/// Extract the underlying data list from an array/tensor (handles xos.Tensor, dict, or list)
fn get_array_data_list(obj: &PyObjectRef, vm: &VirtualMachine) -> PyResult<Option<PyObjectRef>> {
    // Tensor: get_attr("_data") returns the inner dict; dict["_data"] is the list
    if let Ok(data_attr) = obj.get_attr("_data", vm) {
        if let Ok(inner_dict) = data_attr
            .clone()
            .downcast::<rustpython_vm::builtins::PyDict>()
        {
            if let Ok(list_obj) = inner_dict.get_item("_data", vm) {
                if list_obj
                    .downcast_ref::<rustpython_vm::builtins::PyList>()
                    .is_some()
                {
                    return Ok(Some(list_obj));
                }
            }
        }
        // _data is already the list
        if data_attr
            .downcast_ref::<rustpython_vm::builtins::PyList>()
            .is_some()
        {
            return Ok(Some(data_attr));
        }
    }
    // Raw dict
    if let Ok(dict) = obj.clone().downcast::<rustpython_vm::builtins::PyDict>() {
        if let Ok(list_obj) = dict.get_item("_data", vm) {
            if list_obj
                .downcast_ref::<rustpython_vm::builtins::PyList>()
                .is_some()
            {
                return Ok(Some(list_obj));
            }
        }
    }
    // Plain list
    if obj
        .downcast_ref::<rustpython_vm::builtins::PyList>()
        .is_some()
    {
        return Ok(Some(obj.clone()));
    }
    Ok(None)
}

/// xos.ops.convolve(image, kernel, padding="same")
/// 2D convolution via Burn (WGPU on native, NdArray on wasm).
///
/// - image: frame.tensor (read from current frame buffer context)
/// - kernel: 3D array [height, width, channels] - e.g., KxKx3 for RGB
/// - padding: "same" (default) maintains image dimensions
///
/// Returns raw float output as shape [height, width, 3] (RGB, no alpha).
/// Caller is responsible for any display-space mapping/clamping.
/// Note: Automatically detects kernel size from array length
pub fn convolve(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    let inplace = args
        .kwargs
        .get("inplace")
        .and_then(|v| v.clone().try_into_value::<bool>(vm).ok())
        .or_else(|| {
            args.kwargs
                .get("direct")
                .and_then(|v| v.clone().try_into_value::<bool>(vm).ok())
        })
        .unwrap_or(false);
    let stride = args
        .kwargs
        .get("stride")
        .and_then(|v| v.clone().try_into_value::<i32>(vm).ok())
        .unwrap_or(1)
        .max(1) as usize;

    if args_vec.len() < 2 {
        return Err(vm.new_type_error(
            "convolve() requires at least 2 arguments (image, kernel)".to_string(),
        ));
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

    let kernel_nchw = kernel_hwc_to_nchw(&kernel, kernel_size);
    let stride_pair = [stride, stride];

    #[cfg(not(target_arch = "wasm32"))]
    if try_convolve_on_frame_gpu(&kernel_nchw, kernel_size, stride_pair) {
        if inplace {
            return direct_fill_sentinel(vm);
        }
        let buffer_ptr_opt = crate::rasterizer::CURRENT_FRAME_BUFFER
            .lock()
            .unwrap()
            .as_ref()
            .map(|ptr| ptr.as_ptr());
        let width = *crate::rasterizer::CURRENT_FRAME_WIDTH.lock().unwrap();
        let height = *crate::rasterizer::CURRENT_FRAME_HEIGHT.lock().unwrap();
        if let Some(buffer_ptr) = buffer_ptr_opt {
            let buffer =
                unsafe { std::slice::from_raw_parts(buffer_ptr, width * height * 4) };
            let mut output_rgb = vec![0.0f32; width * height * 3];
            for i in 0..(width * height) {
                let src = i * 4;
                let dst = i * 3;
                output_rgb[dst] = buffer[src] as f32;
                output_rgb[dst + 1] = buffer[src + 1] as f32;
                output_rgb[dst + 2] = buffer[src + 2] as f32;
            }
            return wrap_output_tensor(vm, width, height, &output_rgb);
        }
    }

    // Get the frame buffer from global context
    let buffer_ptr_opt = crate::rasterizer::CURRENT_FRAME_BUFFER
        .lock()
        .unwrap()
        .as_ref()
        .map(|ptr| ptr.as_ptr());
    let width = *crate::rasterizer::CURRENT_FRAME_WIDTH
        .lock()
        .unwrap();
    let height = *crate::rasterizer::CURRENT_FRAME_HEIGHT
        .lock()
        .unwrap();

    let buffer_ptr = buffer_ptr_opt.ok_or_else(|| {
        vm.new_runtime_error(
            "No frame buffer context set. convolve must be called during tick().".to_string(),
        )
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

    // Set up convolution parameters for "same" padding
    let pad = (kernel_size - 1) / 2;

    // Allocate output buffer
    let mut output_nchw = vec![0.0f32; width * height * 3];

    conv2d(
        input_nchw,
        kernel_nchw,
        &mut output_nchw,
        1, // batch
        3, // in_channels
        3, // out_channels
        height as usize,
        width as usize,
        kernel_size,
        kernel_size,
        [stride, stride], // stride
        [pad, pad],       // padding
    );

    // Convert back from NCHW to interleaved RGB floats (row-major NHWC)
    let mut output_rgb = vec![0.0f32; width * height * 3];
    for y in 0..height {
        for x in 0..width {
            let dst_idx = (y * width + x) * 3;
            for c in 0..3 {
                let src_idx = (c * height + y) * width + x;
                output_rgb[dst_idx + c] = output_nchw[src_idx];
            }
        }
    }

    // Fast path: write directly into current frame buffer and return sentinel dict.
    // This avoids creating millions of Python float objects each frame.
    if inplace {
        if stride != 1 {
            return Err(vm.new_value_error("inplace=True currently requires stride=1".to_string()));
        }
        for y in 0..height {
            for x in 0..width {
                let src_idx = (y * width + x) * 3;
                let dst_idx = (y * width + x) * 4;
                for c in 0..3 {
                    let iv = output_rgb[src_idx + c] as i32;
                    buffer[dst_idx + c] = iv.clamp(0, 255) as u8;
                }
                buffer[dst_idx + 3] = 255;
            }
        }

        return direct_fill_sentinel(vm);
    }

    wrap_output_tensor(vm, width, height, &output_rgb)
}

fn wrap_output_tensor(
    vm: &VirtualMachine,
    width: usize,
    height: usize,
    output_rgb: &[f32],
) -> PyResult {
    let py_list: Vec<rustpython_vm::PyObjectRef> = output_rgb
        .iter()
        .map(|&v| vm.ctx.new_float(v as f64).into())
        .collect();

    let tensor_dict = vm.ctx.new_dict();
    tensor_dict.set_item(
        "shape",
        vm.ctx
            .new_tuple(vec![
                vm.ctx.new_int(height).into(),
                vm.ctx.new_int(width).into(),
                vm.ctx.new_int(3).into(),
            ])
            .into(),
        vm,
    )?;
    tensor_dict.set_item("dtype", vm.ctx.new_str("float32").into(), vm)?;
    tensor_dict.set_item(
        "device",
        vm.ctx.new_str(xos_tensor::compute_device_label()).into(),
        vm,
    )?;
    tensor_dict.set_item("_data", vm.ctx.new_list(py_list).into(), vm)?;

    if let Ok(wrapper_class) = vm.builtins.get_attr("Tensor", vm) {
        if let Ok(wrapped) = wrapper_class.call((tensor_dict.clone(),), vm) {
            return Ok(wrapped);
        }
    }

    Ok(tensor_dict.into())
}

/// xos.ops.convolve_depthwise(image, kernel, padding="same")
/// Depthwise 2D convolution via Burn (WGPU on native, NdArray on wasm).
///
/// - image: frame.tensor (read from current frame buffer context)
/// - kernel: 2D array [height, width] = KxK values (applied to each channel separately)
/// - padding: "same" (default) maintains image dimensions
///
/// Returns raw float output as shape [height, width, 3] (RGB, no alpha).
/// Caller is responsible for any display-space mapping/clamping.
/// Note: Automatically detects kernel size from array length
pub fn convolve_depthwise(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    let inplace = args
        .kwargs
        .get("inplace")
        .and_then(|v| v.clone().try_into_value::<bool>(vm).ok())
        .or_else(|| {
            args.kwargs
                .get("direct")
                .and_then(|v| v.clone().try_into_value::<bool>(vm).ok())
        })
        .unwrap_or(false);
    let stride = args
        .kwargs
        .get("stride")
        .and_then(|v| v.clone().try_into_value::<i32>(vm).ok())
        .unwrap_or(1)
        .max(1) as usize;

    if args_vec.len() < 2 {
        return Err(vm.new_type_error(
            "convolve_depthwise() requires at least 2 arguments (image, kernel)".to_string(),
        ));
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

    let stride_pair = [stride, stride];

    #[cfg(not(target_arch = "wasm32"))]
    if try_convolve_depthwise_on_frame_gpu(kernel.clone(), kernel_size, stride_pair) {
        if inplace {
            return direct_fill_sentinel(vm);
        }
        let buffer_ptr_opt = crate::rasterizer::CURRENT_FRAME_BUFFER
            .lock()
            .unwrap()
            .as_ref()
            .map(|ptr| ptr.as_ptr());
        let width = *crate::rasterizer::CURRENT_FRAME_WIDTH.lock().unwrap();
        let height = *crate::rasterizer::CURRENT_FRAME_HEIGHT.lock().unwrap();
        if let Some(buffer_ptr) = buffer_ptr_opt {
            let buffer =
                unsafe { std::slice::from_raw_parts(buffer_ptr, width * height * 4) };
            let mut output_rgb = vec![0.0f32; width * height * 3];
            for i in 0..(width * height) {
                let src = i * 4;
                let dst = i * 3;
                output_rgb[dst] = buffer[src] as f32;
                output_rgb[dst + 1] = buffer[src + 1] as f32;
                output_rgb[dst + 2] = buffer[src + 2] as f32;
            }
            return wrap_output_tensor(vm, width, height, &output_rgb);
        }
    }

    // Get the frame buffer from global context
    let buffer_ptr_opt = crate::rasterizer::CURRENT_FRAME_BUFFER
        .lock()
        .unwrap()
        .as_ref()
        .map(|ptr| ptr.as_ptr());
    let width = *crate::rasterizer::CURRENT_FRAME_WIDTH
        .lock()
        .unwrap();
    let height = *crate::rasterizer::CURRENT_FRAME_HEIGHT
        .lock()
        .unwrap();

    let buffer_ptr = buffer_ptr_opt.ok_or_else(|| {
        vm.new_runtime_error(
            "No frame buffer context set. convolve_depthwise must be called during tick()."
                .to_string(),
        )
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
        input_nchw,
        kernel_depthwise,
        &mut output_nchw,
        1, // batch
        3, // channels
        height as usize,
        width as usize,
        kernel_size,
        kernel_size,
        [stride, stride], // stride
        [pad, pad],       // padding
    );

    // Convert back from NCHW to interleaved RGB floats (row-major NHWC)
    let mut output_rgb = vec![0.0f32; width * height * 3];
    for y in 0..height {
        for x in 0..width {
            let dst_idx = (y * width + x) * 3;
            for c in 0..3 {
                let src_idx = (c * height + y) * width + x;
                output_rgb[dst_idx + c] = output_nchw[src_idx];
            }
        }
    }

    // Fast path: write directly into current frame buffer and return sentinel dict.
    if inplace {
        if stride != 1 {
            return Err(vm.new_value_error("inplace=True currently requires stride=1".to_string()));
        }
        for y in 0..height {
            for x in 0..width {
                let src_idx = (y * width + x) * 3;
                let dst_idx = (y * width + x) * 4;
                for c in 0..3 {
                    let iv = output_rgb[src_idx + c] as i32;
                    buffer[dst_idx + c] = iv.clamp(0, 255) as u8;
                }
                buffer[dst_idx + 3] = 255;
            }
        }

        return direct_fill_sentinel(vm);
    }

    wrap_output_tensor(vm, width, height, &output_rgb)
}
