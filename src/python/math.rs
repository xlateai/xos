use rustpython_vm::{PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs, PyObjectRef};

/// xos.math.log(x) - Natural logarithm (base e)
fn log(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let x: f64 = args.bind(vm)?;
    
    if x <= 0.0 {
        return Err(vm.new_value_error("math domain error: log(x) requires x > 0".to_string()));
    }
    
    let result = x.ln();
    Ok(vm.ctx.new_float(result).into())
}

/// xos.math.sqrt(x) - Square root
fn sqrt(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let x: f64 = args.bind(vm)?;
    
    if x < 0.0 {
        return Err(vm.new_value_error("math domain error: sqrt(x) requires x >= 0".to_string()));
    }
    
    let result = x.sqrt();
    Ok(vm.ctx.new_float(result).into())
}

/// xos.math.pow(x, y) - x raised to the power y
fn pow(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let (x, y): (f64, f64) = args.bind(vm)?;
    let result = x.powf(y);
    Ok(vm.ctx.new_float(result).into())
}

/// xos.math.abs(x) - Absolute value
fn abs(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let x: f64 = args.bind(vm)?;
    let result = x.abs();
    Ok(vm.ctx.new_float(result).into())
}

/// xos.math.sin(x) - Sine (x in radians)
fn sin(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let x: f64 = args.bind(vm)?;
    let result = x.sin();
    Ok(vm.ctx.new_float(result).into())
}

/// xos.math.cos(x) - Cosine (x in radians)
fn cos(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let x: f64 = args.bind(vm)?;
    let result = x.cos();
    Ok(vm.ctx.new_float(result).into())
}

/// xos.math.tan(x) - Tangent (x in radians)
fn tan(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let x: f64 = args.bind(vm)?;
    let result = x.tan();
    Ok(vm.ctx.new_float(result).into())
}

/// xos.math.floor(x) - Floor function
fn floor(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let x: f64 = args.bind(vm)?;
    let result = x.floor();
    Ok(vm.ctx.new_float(result).into())
}

/// xos.math.ceil(x) - Ceiling function
fn ceil(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let x: f64 = args.bind(vm)?;
    let result = x.ceil();
    Ok(vm.ctx.new_float(result).into())
}

/// xos.math.fft(samples) - Fast Fourier Transform
/// Returns tuple of (real_parts, imag_parts) for complex FFT result
fn fft(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.is_empty() {
        return Err(vm.new_type_error("fft() requires 1 argument (samples)".to_string()));
    }
    
    // Parse input samples
    let samples = if let Some(list) = args_vec[0].downcast_ref::<rustpython_vm::builtins::PyList>() {
        let vec = list.borrow_vec();
        vec.iter()
            .map(|item| item.clone().try_into_value::<f64>(vm))
            .collect::<Result<Vec<_>, _>>()?
    } else if let Some(dict) = args_vec[0].downcast_ref::<rustpython_vm::builtins::PyDict>() {
        // Handle xos.tensor / xos.array format (dict with _data)
        if let Ok(data_obj) = dict.get_item("_data", vm) {
            if let Some(data_list) = data_obj.downcast_ref::<rustpython_vm::builtins::PyList>() {
                let vec = data_list.borrow_vec();
                vec.iter()
                    .map(|item| item.clone().try_into_value::<f64>(vm))
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                return Err(vm.new_type_error("Invalid tensor format".to_string()));
            }
        } else {
            return Err(vm.new_type_error("Tensor missing _data field".to_string()));
        }
    } else {
        return Err(vm.new_type_error("fft() requires a list or array".to_string()));
    };
    
    let n = samples.len();
    
    // Cooley-Tukey FFT (radix-2, requires power of 2)
    if n == 0 || (n & (n - 1)) != 0 {
        return Err(vm.new_value_error(format!("FFT requires power-of-2 length, got {}", n)));
    }
    
    // Convert real samples to complex (real, imag)
    let mut real: Vec<f64> = samples;
    let mut imag: Vec<f64> = vec![0.0; n];
    
    // Bit-reversal permutation
    let mut j = 0;
    for i in 0..n - 1 {
        if i < j {
            real.swap(i, j);
            imag.swap(i, j);
        }
        let mut k = n / 2;
        while k <= j {
            j -= k;
            k /= 2;
        }
        j += k;
    }
    
    // Cooley-Tukey decimation-in-time
    let mut length = 2;
    while length <= n {
        let angle = -2.0 * std::f64::consts::PI / length as f64;
        let wlen_r = angle.cos();
        let wlen_i = angle.sin();
        
        let mut i = 0;
        while i < n {
            let mut w_r = 1.0;
            let mut w_i = 0.0;
            
            for j in 0..length / 2 {
                let u_r = real[i + j];
                let u_i = imag[i + j];
                let v_r = real[i + j + length / 2];
                let v_i = imag[i + j + length / 2];
                
                let t_r = w_r * v_r - w_i * v_i;
                let t_i = w_r * v_i + w_i * v_r;
                
                real[i + j] = u_r + t_r;
                imag[i + j] = u_i + t_i;
                real[i + j + length / 2] = u_r - t_r;
                imag[i + j + length / 2] = u_i - t_i;
                
                let w_r_tmp = w_r;
                w_r = w_r * wlen_r - w_i * wlen_i;
                w_i = w_r_tmp * wlen_i + w_i * wlen_r;
            }
            
            i += length;
        }
        
        length *= 2;
    }
    
    // Convert to Python lists
    let real_list: Vec<PyObjectRef> = real.iter().map(|&r| vm.ctx.new_float(r).into()).collect();
    let imag_list: Vec<PyObjectRef> = imag.iter().map(|&i| vm.ctx.new_float(i).into()).collect();
    
    // Return tuple (real, imag)
    Ok(vm.ctx.new_tuple(vec![
        vm.ctx.new_list(real_list).into(),
        vm.ctx.new_list(imag_list).into(),
    ]).into())
}

/// Create the math module
pub fn make_math_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.math", vm.ctx.new_dict(), None);
    
    // Add math functions
    let _ = module.set_attr("log", vm.new_function("log", log), vm);
    let _ = module.set_attr("sqrt", vm.new_function("sqrt", sqrt), vm);
    let _ = module.set_attr("pow", vm.new_function("pow", pow), vm);
    let _ = module.set_attr("abs", vm.new_function("abs", abs), vm);
    let _ = module.set_attr("sin", vm.new_function("sin", sin), vm);
    let _ = module.set_attr("cos", vm.new_function("cos", cos), vm);
    let _ = module.set_attr("tan", vm.new_function("tan", tan), vm);
    let _ = module.set_attr("floor", vm.new_function("floor", floor), vm);
    let _ = module.set_attr("ceil", vm.new_function("ceil", ceil), vm);
    let _ = module.set_attr("fft", vm.new_function("fft", fft), vm);
    
    // Add common constants
    let _ = module.set_attr("pi", vm.ctx.new_float(std::f64::consts::PI), vm);
    let _ = module.set_attr("e", vm.ctx.new_float(std::f64::consts::E), vm);
    
    module
}

