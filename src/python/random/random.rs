#[cfg(feature = "python")]
use rustpython_vm::{PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs};

/// xos.random.uniform(min, max) - returns a random float between min and max
fn uniform(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let (min, max): (f64, f64) = args.bind(vm)?;
    
    #[cfg(target_arch = "wasm32")]
    {
        let random = js_sys::Math::random();
        let value = min + random * (max - min);
        Ok(vm.ctx.new_float(value).into())
    }
    
    #[cfg(not(target_arch = "wasm32"))]
    {
        use rand::Rng;
        let mut rng = rand::rng();
        let value: f64 = rng.random_range(min..max);
        Ok(vm.ctx.new_float(value).into())
    }
}

/// Create the random submodule
pub fn make_random_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("random", vm.ctx.new_dict(), None);
    
    // Add uniform function
    module.set_attr("uniform", vm.new_function("uniform", uniform), vm).unwrap();
    
    module
}

