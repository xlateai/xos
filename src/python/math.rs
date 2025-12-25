use rustpython_vm::{PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs};

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
    
    // Add common constants
    let _ = module.set_attr("pi", vm.ctx.new_float(std::f64::consts::PI), vm);
    let _ = module.set_attr("e", vm.ctx.new_float(std::f64::consts::E), vm);
    
    module
}

