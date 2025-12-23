use rustpython_vm::{PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs, PyObjectRef};

/// xos.random.uniform(min, max, shape=None) - returns a random float or array
/// 
/// If shape is None (default), returns a single random float between min and max
/// If shape is provided as a tuple, returns a list of random u8 values (0-255) for image data
fn uniform(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    
    if args_vec.len() < 2 {
        return Err(vm.new_type_error("uniform() requires at least 2 arguments (min, max)".to_string()));
    }
    
    let min: f64 = args_vec[0].clone().try_into_value(vm)?;
    let max: f64 = args_vec[1].clone().try_into_value(vm)?;
    
    // Check if shape argument was provided (as 3rd positional arg or as kwarg)
    let shape_arg = if args_vec.len() > 2 {
        Some(&args_vec[2])
    } else {
        // Check kwargs for 'shape' key
        args.kwargs.iter().find_map(|(k, v)| {
            if k == "shape" {
                Some(v)
            } else {
                None
            }
        })
    };
    
    // If no shape, return a single float
    if shape_arg.is_none() || vm.is_none(shape_arg.unwrap()) {
        #[cfg(target_arch = "wasm32")]
        {
            let random = js_sys::Math::random();
            let value = min + random * (max - min);
            return Ok(vm.ctx.new_float(value).into());
        }
        
        #[cfg(not(target_arch = "wasm32"))]
        {
            use rand::Rng;
            let mut rng = rand::rng();
            let value: f64 = rng.random_range(min..max);
            return Ok(vm.ctx.new_float(value).into());
        }
    }
    
    // Shape provided - generate array of random u8 values
    let shape_obj = shape_arg.unwrap();
    let shape_tuple = shape_obj.downcast_ref::<rustpython_vm::builtins::PyTuple>()
        .ok_or_else(|| vm.new_type_error("shape must be a tuple".to_string()))?;
    
    let shape: Vec<usize> = shape_tuple.as_slice().iter()
        .map(|s| s.clone().try_into_value::<i32>(vm).map(|i| i as usize))
        .collect::<Result<Vec<_>, _>>()?;
    
    let total_elements: usize = shape.iter().product();
    
    // Generate random u8 values (0-255) for image data
    let random_data: Vec<u8>;
    
    #[cfg(target_arch = "wasm32")]
    {
        random_data = (0..total_elements)
            .map(|_| {
                let random = js_sys::Math::random();
                let value = min + random * (max - min);
                value.clamp(0.0, 255.0) as u8
            })
            .collect();
    }
    
    #[cfg(not(target_arch = "wasm32"))]
    {
        use rand::Rng;
        let mut rng = rand::rng();
        random_data = (0..total_elements)
            .map(|_| {
                let value: f64 = rng.random_range(min..max);
                value.clamp(0.0, 255.0) as u8
            })
            .collect();
    }
    
    // Convert to Python list
    let py_list: Vec<PyObjectRef> = random_data.iter()
        .map(|&b| vm.ctx.new_int(b).into())
        .collect();
    
    Ok(vm.ctx.new_list(py_list).into())
}

/// Create the random submodule
pub fn make_random_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("random", vm.ctx.new_dict(), None);
    
    // Add uniform function
    module.set_attr("uniform", vm.new_function("uniform", uniform), vm).unwrap();
    
    module
}

