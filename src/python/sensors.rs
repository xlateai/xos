use rustpython_vm::{PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs};

/// xos.sensors.magnetometer.init() - Initialize the magnetometer
fn magnetometer_init(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    #[cfg(target_os = "ios")]
    {
        let result = unsafe { xos_magnetometer_init() };
        if result == 0 {
            Ok(vm.ctx.none())
        } else {
            Err(vm.new_runtime_error(format!("Failed to initialize magnetometer (error code: {})", result)))
        }
    }
    
    #[cfg(not(target_os = "ios"))]
    {
        Err(vm.new_runtime_error("Magnetometer only available on iOS".to_string()))
    }
}

/// xos.sensors.magnetometer.get_latest() - Get the latest reading
/// Returns a dict with x, y, z fields, or None if no data available
fn magnetometer_get_latest(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
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
                // Success - return dict with readings
                let dict = vm.ctx.new_dict();
                dict.set_item("x", vm.ctx.new_float(x).into(), vm)?;
                dict.set_item("y", vm.ctx.new_float(y).into(), vm)?;
                dict.set_item("z", vm.ctx.new_float(z).into(), vm)?;
                Ok(dict.into())
            }
            1 => {
                // No data available
                Ok(vm.ctx.none())
            }
            _ => {
                Err(vm.new_runtime_error("Magnetometer not initialized or error occurred".to_string()))
            }
        }
    }
    
    #[cfg(not(target_os = "ios"))]
    {
        Err(vm.new_runtime_error("Magnetometer only available on iOS".to_string()))
    }
}

/// xos.sensors.magnetometer.drain_readings() - Get all readings since last call
/// Returns a list of dicts with x, y, z fields
fn magnetometer_drain_readings(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    #[cfg(target_os = "ios")]
    {
        const MAX_READINGS: usize = 1024;
        let mut x_array = vec![0.0f64; MAX_READINGS];
        let mut y_array = vec![0.0f64; MAX_READINGS];
        let mut z_array = vec![0.0f64; MAX_READINGS];
        
        let count = unsafe {
            xos_magnetometer_drain_readings(
                x_array.as_mut_ptr(),
                y_array.as_mut_ptr(),
                z_array.as_mut_ptr(),
                MAX_READINGS,
            )
        };
        
        if count < 0 {
            return Err(vm.new_runtime_error("Magnetometer not initialized or error occurred".to_string()));
        }
        
        // Build list of reading dicts
        let mut readings = Vec::new();
        for i in 0..(count as usize) {
            let dict = vm.ctx.new_dict();
            dict.set_item("x", vm.ctx.new_float(x_array[i]).into(), vm)?;
            dict.set_item("y", vm.ctx.new_float(y_array[i]).into(), vm)?;
            dict.set_item("z", vm.ctx.new_float(z_array[i]).into(), vm)?;
            readings.push(dict.into());
        }
        
        Ok(vm.ctx.new_list(readings).into())
    }
    
    #[cfg(not(target_os = "ios"))]
    {
        Err(vm.new_runtime_error("Magnetometer only available on iOS".to_string()))
    }
}

/// xos.sensors.magnetometer.get_total_readings() - Get total count of readings received
fn magnetometer_get_total_readings(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    #[cfg(target_os = "ios")]
    {
        let count = unsafe { xos_magnetometer_get_total_readings() };
        Ok(vm.ctx.new_int(count as i64).into())
    }
    
    #[cfg(not(target_os = "ios"))]
    {
        Err(vm.new_runtime_error("Magnetometer only available on iOS".to_string()))
    }
}

/// xos.sensors.magnetometer.cleanup() - Cleanup magnetometer
fn magnetometer_cleanup(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    #[cfg(target_os = "ios")]
    {
        unsafe { xos_magnetometer_cleanup() };
        Ok(vm.ctx.none())
    }
    
    #[cfg(not(target_os = "ios"))]
    {
        Ok(vm.ctx.none())
    }
}

/// Create the magnetometer submodule
fn make_magnetometer_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.sensors.magnetometer", vm.ctx.new_dict(), None);
    module.set_attr("init", vm.new_function("init", magnetometer_init), vm).unwrap();
    module.set_attr("get_latest", vm.new_function("get_latest", magnetometer_get_latest), vm).unwrap();
    module.set_attr("drain_readings", vm.new_function("drain_readings", magnetometer_drain_readings), vm).unwrap();
    module.set_attr("get_total_readings", vm.new_function("get_total_readings", magnetometer_get_total_readings), vm).unwrap();
    module.set_attr("cleanup", vm.new_function("cleanup", magnetometer_cleanup), vm).unwrap();
    module
}

/// Create the sensors module
pub fn make_sensors_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.sensors", vm.ctx.new_dict(), None);
    
    // Add magnetometer submodule
    let magnetometer_module = make_magnetometer_module(vm);
    module.set_attr("magnetometer", magnetometer_module, vm).unwrap();
    
    module
}

// FFI declarations
#[cfg(target_os = "ios")]
extern "C" {
    fn xos_magnetometer_init() -> i32;
    fn xos_magnetometer_get_latest(x: *mut f64, y: *mut f64, z: *mut f64) -> i32;
    fn xos_magnetometer_drain_readings(x_array: *mut f64, y_array: *mut f64, z_array: *mut f64, max_count: usize) -> i32;
    fn xos_magnetometer_get_total_readings() -> u64;
    fn xos_magnetometer_cleanup();
}

