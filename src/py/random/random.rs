use rustpython_vm::{PyObjectRef, PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs};

#[cfg(any(target_os = "macos", target_os = "ios"))]
use std::sync::OnceLock;

#[cfg(any(target_os = "macos", target_os = "ios"))]
static METAL_RANDOM_STATE: OnceLock<Option<MetalRandomState>> = OnceLock::new();

#[cfg(any(target_os = "macos", target_os = "ios"))]
struct MetalRandomState {
    device: metal::Device,
    command_queue: metal::CommandQueue,
    pipeline: metal::ComputePipelineState,
}

/// Try to fill buffer with random data using Metal GPU (iOS/macOS only)
/// Returns true if successful, false if Metal unavailable (falls back to CPU)
#[cfg(any(target_os = "macos", target_os = "ios"))]
fn try_fill_random_metal(buffer: &mut [u8], low: f64, high: f64) -> bool {
    // Initialize Metal state lazily
    let metal_state = METAL_RANDOM_STATE.get_or_init(|| {
        let device = metal::Device::system_default()?;
        let command_queue = device.new_command_queue();
        
        // Metal shader for random noise generation
        let shader_source = r#"
        #include <metal_stdlib>
        using namespace metal;
        
        // Simple hash-based random number generator
        uint hash(uint x) {
            x ^= x >> 16;
            x *= 0x7feb352dU;
            x ^= x >> 15;
            x *= 0x846ca68bU;
            x ^= x >> 16;
            return x;
        }
        
        kernel void fill_random(
            device uchar* buffer [[buffer(0)]],
            constant uint& seed [[buffer(1)]],
            constant float& low [[buffer(2)]],
            constant float& high [[buffer(3)]],
            constant uint& length [[buffer(4)]],
            uint gid [[thread_position_in_grid]]
        ) {
            if (gid >= length) return;
            
            // Generate random value
            uint rand_val = hash(gid + seed);
            float normalized = float(rand_val) / float(UINT_MAX);
            float value = low + normalized * (high - low);
            buffer[gid] = uchar(clamp(value, 0.0f, 255.0f));
        }
        "#;
        
        let library = device.new_library_with_source(shader_source, &metal::CompileOptions::new()).ok()?;
        let function = library.get_function("fill_random", None).ok()?;
        let pipeline = device.new_compute_pipeline_state_with_function(&function).ok()?;
        
        Some(MetalRandomState {
            device,
            command_queue,
            pipeline,
        })
    });
    
    let state = match metal_state.as_ref() {
        Some(s) => s,
        None => return false, // Metal not available
    };
    
    // Create Metal buffer from our existing buffer
    let buffer_len = buffer.len();
    let metal_buffer = state.device.new_buffer_with_data(
        buffer.as_ptr() as *const _,
        buffer_len as u64,
        metal::MTLResourceOptions::StorageModeShared,
    );
    
    // Prepare parameters
    let seed: u32 = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() & 0xFFFFFFFF) as u32;
    let low_f32 = low as f32;
    let high_f32 = high as f32;
    let length = buffer_len as u32;
    
    // Execute compute shader
    let command_buffer = state.command_queue.new_command_buffer();
    let encoder = command_buffer.new_compute_command_encoder();
    
    encoder.set_compute_pipeline_state(&state.pipeline);
    encoder.set_buffer(0, Some(&metal_buffer), 0);
    encoder.set_bytes(1, std::mem::size_of::<u32>() as u64, &seed as *const u32 as *const _);
    encoder.set_bytes(2, std::mem::size_of::<f32>() as u64, &low_f32 as *const f32 as *const _);
    encoder.set_bytes(3, std::mem::size_of::<f32>() as u64, &high_f32 as *const f32 as *const _);
    encoder.set_bytes(4, std::mem::size_of::<u32>() as u64, &length as *const u32 as *const _);
    
    // Calculate thread group sizes
    let thread_group_size = metal::MTLSize::new(256, 1, 1);
    let thread_groups = metal::MTLSize::new(
        ((buffer_len + 255) / 256) as u64,
        1,
        1,
    );
    
    encoder.dispatch_thread_groups(thread_groups, thread_group_size);
    encoder.end_encoding();
    
    command_buffer.commit();
    command_buffer.wait_until_completed();
    
    // Copy results back from Metal buffer to our buffer
    unsafe {
        std::ptr::copy_nonoverlapping(
            metal_buffer.contents() as *const u8,
            buffer.as_mut_ptr(),
            buffer_len,
        );
    }
    
    true
}

/// xos.random.uniform(low=0.0, high=1.0, shape=None, dtype=None) - returns a random float or array
/// 
/// Extract f64 from Python int or float
fn parse_f64(obj: &PyObjectRef, vm: &VirtualMachine) -> PyResult<f64> {
    if let Ok(f) = obj.clone().try_into_value::<f64>(vm) {
        return Ok(f);
    }
    if let Ok(i) = obj.clone().try_into_value::<i64>(vm) {
        return Ok(i as f64);
    }
    Err(vm.new_type_error("Expected a number (int or float)".to_string()))
}

/// If shape is None (default), returns a single random float between low and high
/// If shape is provided as a tuple, returns an array of random values
/// dtype can be specified (default: inferred from context - float32 for kernels, uint8 for images)
fn uniform(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    
    // Parse low parameter (default: 0.0) - accept int or float
    let low: f64 = if !args_vec.is_empty() {
        parse_f64(&args_vec[0], vm)?
    } else if let Some(low_kwarg) = args.kwargs.get("low") {
        parse_f64(low_kwarg, vm)?
    } else {
        0.0
    };
    
    // Parse high parameter (default: 1.0) - accept int or float
    let high: f64 = if args_vec.len() > 1 {
        parse_f64(&args_vec[1], vm)?
    } else if let Some(high_kwarg) = args.kwargs.get("high") {
        parse_f64(high_kwarg, vm)?
    } else {
        1.0
    };
    
    // Check if shape argument was provided (as 3rd positional arg or as kwarg)
    let shape_arg = if args_vec.len() > 2 && !vm.is_none(&args_vec[2]) {
        Some(&args_vec[2])
    } else {
        // Check kwargs for 'shape' key
        args.kwargs.iter().find_map(|(k, v)| {
            if k == "shape" && !vm.is_none(v) {
                Some(v)
            } else {
                None
            }
        })
    };
    
    // Check for dtype argument
    let dtype_arg = args.kwargs.iter().find_map(|(k, v)| {
        if k == "dtype" {
            Some(v)
        } else {
            None
        }
    });
    
    // If no shape, return a single float
    if shape_arg.is_none() || vm.is_none(shape_arg.unwrap()) {
        #[cfg(target_arch = "wasm32")]
        {
            let random = js_sys::Math::random();
            let value = low + random * (high - low);
            return Ok(vm.ctx.new_float(value).into());
        }
        
        #[cfg(not(target_arch = "wasm32"))]
        {
            use rand::Rng;
            let mut rng = rand::rng();
            let value: f64 = rng.random_range(low..high);
            return Ok(vm.ctx.new_float(value).into());
        }
    }
    
    // Shape provided - generate array of random values
    let shape_obj = shape_arg.unwrap();
    let shape_tuple = shape_obj.downcast_ref::<rustpython_vm::builtins::PyTuple>()
        .ok_or_else(|| vm.new_type_error("shape must be a tuple".to_string()))?;
    
    let shape: Vec<usize> = shape_tuple.as_slice().iter()
        .map(|s| s.clone().try_into_value::<i32>(vm).map(|i| i as usize))
        .collect::<Result<Vec<_>, _>>()?;
    
    let total_elements: usize = shape.iter().product();
    
    // Determine if we should generate floats or integers
    // Default: float32 (for audio compatibility), unless dtype explicitly specifies uint8
    let use_float = if let Some(dtype_obj) = dtype_arg {
        // Check if dtype has a 'name' attribute
        if let Ok(name_attr) = dtype_obj.get_attr("name", vm) {
            if let Ok(s) = name_attr.str(vm) {
                let name = s.to_string();
                // Use float unless explicitly uint8 or int
                !name.contains("uint") && !name.contains("int")
            } else {
                true
            }
        } else {
            true
        }
    } else {
        // Default to float32 for audio and general use
        true
    };
    
    if use_float {
        // Generate random f32 values
        let random_data: Vec<f32>;
        
        #[cfg(target_arch = "wasm32")]
        {
            random_data = (0..total_elements)
                .map(|_| {
                    let random = js_sys::Math::random();
                    (low + random * (high - low)) as f32
                })
                .collect();
        }
        
        #[cfg(not(target_arch = "wasm32"))]
        {
            use rand::Rng;
            let mut rng = rand::rng();
            random_data = (0..total_elements)
                .map(|_| {
                    let value: f64 = rng.random_range(low..high);
                    value as f32
                })
                .collect();
        }
        
        // Create xos.tensor backed by Rust memory
        use crate::python_api::tensors::PyTensor;
        use crate::python_api::dtypes::DType;
        
        let py_tensor = PyTensor::new(random_data, shape.clone());
        let dict = py_tensor.to_py_dict(vm, DType::Float32)?;
        
        // Wrap in _TensorWrapper for nice display and compatibility
        if let Ok(wrapper_class) = vm.builtins.get_attr("_TensorWrapper", vm) {
            if let Ok(wrapped) = wrapper_class.call((dict.clone(),), vm) {
                return Ok(wrapped);
            }
        }
        
        Ok(dict.into())
    } else {
        // Generate random u8 values (0-255) for image data
        let random_data: Vec<f32>;
        
        #[cfg(target_arch = "wasm32")]
        {
            random_data = (0..total_elements)
                .map(|_| {
                    let random = js_sys::Math::random();
                    let value = low + random * (high - low);
                    value.clamp(0.0, 255.0) as f32
                })
                .collect();
        }
        
        #[cfg(not(target_arch = "wasm32"))]
        {
            use rand::Rng;
            let mut rng = rand::rng();
            random_data = (0..total_elements)
                .map(|_| {
                    let value: f64 = rng.random_range(low..high);
                    value.clamp(0.0, 255.0) as f32
                })
                .collect();
        }
        
        // Create xos.tensor backed by Rust memory (stored as f32, displayed as u8)
        use crate::python_api::tensors::PyTensor;
        use crate::python_api::dtypes::DType;
        
        let py_tensor = PyTensor::new(random_data, shape.clone());
        let dict = py_tensor.to_py_dict(vm, DType::UInt8)?;
        
        // Wrap in _TensorWrapper for nice display and compatibility
        if let Ok(wrapper_class) = vm.builtins.get_attr("_TensorWrapper", vm) {
            if let Ok(wrapped) = wrapper_class.call((dict.clone(),), vm) {
                return Ok(wrapped);
            }
        }
        
        Ok(dict.into())
    }
}

/// xos.random.uniform_fill(array, low, high) - fill array directly with random values (ZERO COPY)
/// 
/// Fills the frame buffer array directly with random values without any Python allocations.
/// This is the fast path for operations like: array[:] = xos.random.uniform_fill(array, 0, 255)
fn uniform_fill(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    
    if args_vec.len() != 3 {
        return Err(vm.new_type_error(format!(
            "uniform_fill() takes exactly 3 arguments ({} given)",
            args_vec.len()
        )));
    }
    
    let _array_dict = &args_vec[0]; // Array dict (not used, we access buffer directly)
    let low: f64 = parse_f64(&args_vec[1], vm)?;
    let high: f64 = parse_f64(&args_vec[2], vm)?;
    
    // Get the frame buffer from global context
    let buffer_guard = crate::python_api::rasterizer::CURRENT_FRAME_BUFFER.lock().unwrap();
    let width = *crate::python_api::rasterizer::CURRENT_FRAME_WIDTH.lock().unwrap();
    let height = *crate::python_api::rasterizer::CURRENT_FRAME_HEIGHT.lock().unwrap();
    
    let buffer_ptr = buffer_guard.as_ref()
        .ok_or_else(|| {
            vm.new_runtime_error("No frame buffer context set. uniform_fill must be called during tick().".to_string())
        })?;
    
    let buffer_len = width * height * 4;
    // Access the inner pointer through pattern matching or deref
    let ptr = match buffer_ptr {
        crate::python_api::rasterizer::FrameBufferPtr(p) => *p,
    };
    let buffer = unsafe { std::slice::from_raw_parts_mut(ptr, buffer_len) };
    drop(buffer_guard);
    
    // Fill buffer directly with random values
    #[cfg(target_arch = "wasm32")]
    {
        for pixel in buffer.iter_mut() {
            let random = js_sys::Math::random();
            let value = low + random * (high - low);
            *pixel = value.clamp(0.0, 255.0) as u8;
        }
    }
    
    // Metal GPU path for iOS/macOS - 10x+ faster than CPU
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        if try_fill_random_metal(buffer, low, high) {
            // Successfully filled on GPU - return immediately
            let sentinel = vm.ctx.new_dict();
            sentinel.set_item("_direct_fill", vm.ctx.new_bool(true).into(), vm)?;
            return Ok(sentinel.into());
        }
        // If Metal fails, fall through to CPU path
    }
    
    #[cfg(not(target_arch = "wasm32"))]
    {
        // OPTIMIZATION 1: Parallel CPU generation using rayon
        // OPTIMIZATION 2: Generate u64s and split into bytes for 8x fewer RNG calls
        use rayon::prelude::*;
        use rand::Rng;
        
        let scale = (high - low) / 255.0;
        let offset = low;
        
        // Split buffer into chunks for parallel processing
        // Use chunk size that's multiple of 8 for u64 efficiency
        const CHUNK_SIZE: usize = 64 * 1024; // 64KB chunks for good cache locality
        
        buffer.par_chunks_mut(CHUNK_SIZE).for_each(|chunk| {
            let mut rng = rand::rng();
            
            // Process 8 bytes at a time using u64
            let chunks = chunk.len() / 8;
            let remainder = chunk.len() % 8;
            
            for i in 0..chunks {
                let random_u64: u64 = rng.random();
                let bytes = random_u64.to_le_bytes();
                let start = i * 8;
                for j in 0..8 {
                    let normalized = bytes[j] as f64;
                    let value = offset + normalized * scale;
                    chunk[start + j] = value.clamp(0.0, 255.0) as u8;
                }
            }
            
            // Handle remaining bytes
            if remainder > 0 {
                let random_u64: u64 = rng.random();
                let bytes = random_u64.to_le_bytes();
                let start = chunks * 8;
                for j in 0..remainder {
                    let normalized = bytes[j] as f64;
                    let value = offset + normalized * scale;
                    chunk[start + j] = value.clamp(0.0, 255.0) as u8;
                }
            }
        });
    }
    
    // Return sentinel dict to signal that data is already in buffer
    let sentinel = vm.ctx.new_dict();
    sentinel.set_item("_direct_fill", vm.ctx.new_bool(true).into(), vm)?;
    Ok(sentinel.into())
}

/// xos.random.randint(a, b) -> int in the inclusive range [a, b]
fn randint(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() != 2 {
        return Err(vm.new_type_error(format!(
            "randint() takes exactly 2 arguments ({} given)",
            args_vec.len()
        )));
    }

    let low: i64 = args_vec[0].clone().try_into_value(vm)?;
    let high: i64 = args_vec[1].clone().try_into_value(vm)?;
    if low > high {
        return Err(vm.new_value_error("randint() low must be <= high".to_string()));
    }

    #[cfg(target_arch = "wasm32")]
    {
        let span = (high - low + 1) as f64;
        let value = low + (js_sys::Math::random() * span).floor() as i64;
        return Ok(vm.ctx.new_int(value).into());
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        use rand::Rng;
        let mut rng = rand::rng();
        let value: i64 = rng.random_range(low..=high);
        Ok(vm.ctx.new_int(value).into())
    }
}

/// xos.random.choice(seq, size=None)
/// - choice(seq) returns one random element
/// - choice(seq, size) returns a list with `size` random elements (with replacement)
fn choice(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.is_empty() || args_vec.len() > 2 {
        return Err(vm.new_type_error(format!(
            "choice() takes 1 or 2 arguments ({} given)",
            args_vec.len()
        )));
    }

    let seq = &args_vec[0];
    let size_opt: Option<usize> = if args_vec.len() == 2 {
        let size_raw = args_vec[1].clone().try_into_value::<i64>(vm)?;
        if size_raw < 0 {
            return Err(vm.new_value_error("choice() size must be >= 0".to_string()));
        }
        Some(size_raw as usize)
    } else {
        None
    };

    // Support strings directly for ergonomic character sampling.
    if let Ok(s) = seq.clone().try_into_value::<String>(vm) {
        let chars: Vec<char> = s.chars().collect();
        if chars.is_empty() {
            return Err(vm.new_index_error("choice() cannot choose from an empty sequence".to_string()));
        }

        let sample_char = || -> char {
            #[cfg(target_arch = "wasm32")]
            {
                let idx = (js_sys::Math::random() * chars.len() as f64).floor() as usize;
                chars[idx]
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                use rand::Rng;
                let mut rng = rand::rng();
                chars[rng.random_range(0..chars.len())]
            }
        };

        if let Some(size) = size_opt {
            let mut out = Vec::with_capacity(size);
            for _ in 0..size {
                out.push(vm.ctx.new_str(sample_char().to_string()).into());
            }
            return Ok(vm.ctx.new_list(out).into());
        }

        return Ok(vm.ctx.new_str(sample_char().to_string()).into());
    }

    // Generic sequence fallback using __len__ and __getitem__.
    let len_obj = vm.call_method(seq, "__len__", ())?;
    let len: i64 = len_obj.try_into_value(vm)?;
    if len <= 0 {
        return Err(vm.new_index_error("choice() cannot choose from an empty sequence".to_string()));
    }

    let pick_index = || -> i64 {
        #[cfg(target_arch = "wasm32")]
        {
            (js_sys::Math::random() * len as f64).floor() as i64
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            use rand::Rng;
            let mut rng = rand::rng();
            rng.random_range(0..len)
        }
    };

    if let Some(size) = size_opt {
        let mut out = Vec::with_capacity(size);
        for _ in 0..size {
            let idx = pick_index();
            let item = vm.call_method(seq, "__getitem__", (idx,))?;
            out.push(item);
        }
        return Ok(vm.ctx.new_list(out).into());
    }

    let idx = pick_index();
    vm.call_method(seq, "__getitem__", (idx,))
}

/// Create the random submodule
pub fn make_random_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("random", vm.ctx.new_dict(), None);
    
    // Add uniform function
    module.set_attr("uniform", vm.new_function("uniform", uniform), vm).unwrap();
    module.set_attr("randint", vm.new_function("randint", randint), vm).unwrap();
    module.set_attr("choice", vm.new_function("choice", choice), vm).unwrap();
    
    // Add uniform_fill function (zero-copy direct fill)
    module.set_attr("uniform_fill", vm.new_function("uniform_fill", uniform_fill), vm).unwrap();
    
    module
}

