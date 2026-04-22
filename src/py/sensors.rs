use rustpython_vm::{PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs};

/// xos.sensors.magnetometer() - Initialize and return a magnetometer instance
fn magnetometer_new(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    #[cfg(target_os = "ios")]
    {
        // Initialize the magnetometer
        let result = unsafe { xos_magnetometer_init() };
        if result != 0 {
            return Err(vm.new_runtime_error(format!("Failed to initialize magnetometer (error code: {})", result)));
        }
        
        // Create a Python class with a read method
        let code = r#"
class Magnetometer:
    def read(self):
        import xos
        return xos.sensors._magnetometer_read()

_mag_instance = Magnetometer()
"#;
        let scope = vm.new_scope_with_builtins();
        vm.run_code_string(scope.clone(), code, "<magnetometer>".to_string())?;
        
        // Get the instance from the scope
        let instance = scope.globals.get_item("_mag_instance", vm)?;
        Ok(instance)
    }
    
    #[cfg(not(target_os = "ios"))]
    {
        Err(vm.new_runtime_error("Magnetometer only available on iOS".to_string()))
    }
}

/// magnetometer.read() - Read current magnetometer values (non-blocking)
fn magnetometer_read(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    #[cfg(target_os = "ios")]
    {
        let mut x = 0.0;
        let mut y = 0.0;
        let mut z = 0.0;
        
        let result = unsafe {
            xos_magnetometer_get_latest(&mut x, &mut y, &mut z)
        };
        
        match result {
            0 => {
                // Success - return tuple (x, y, z)
                let tuple = vm.ctx.new_tuple(vec![
                    vm.ctx.new_float(x).into(),
                    vm.ctx.new_float(y).into(),
                    vm.ctx.new_float(z).into(),
                ]);
                Ok(tuple.into())
            }
            1 => {
                // No data available yet - return last known values (or zeros initially)
                // This is non-blocking which is critical for viewport apps
                let tuple = vm.ctx.new_tuple(vec![
                    vm.ctx.new_float(x).into(),
                    vm.ctx.new_float(y).into(),
                    vm.ctx.new_float(z).into(),
                ]);
                Ok(tuple.into())
            }
            _ => {
                Err(vm.new_runtime_error("Magnetometer error".to_string()))
            }
        }
    }
    
    #[cfg(not(target_os = "ios"))]
    {
        Err(vm.new_runtime_error("Magnetometer only available on iOS".to_string()))
    }
}

/// Create the sensors module
pub fn make_sensors_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.sensors", vm.ctx.new_dict(), None);
    
    // Public API: xos.sensors.magnetometer() - creates instance with read() method
    module.set_attr("magnetometer", vm.new_function("magnetometer", magnetometer_new), vm).unwrap();
    
    // Internal function used by Magnetometer.read()
    module.set_attr("_magnetometer_read", vm.new_function("_magnetometer_read", magnetometer_read), vm).unwrap();
    
    module
}

// FFI declarations
#[cfg(target_os = "ios")]
extern "C" {
    fn xos_magnetometer_init() -> i32;
    fn xos_magnetometer_get_latest(x: *mut f64, y: *mut f64, z: *mut f64) -> i32;
    #[allow(dead_code)]
    fn xos_magnetometer_cleanup();
}
