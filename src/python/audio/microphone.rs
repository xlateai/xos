use rustpython_vm::{PyResult, VirtualMachine, function::FuncArgs, PyObjectRef, AsObject};
use crate::audio;
use std::sync::Mutex;
use std::collections::HashSet;
use std::sync::OnceLock;

// Global registry to track all active microphone pointers
static ACTIVE_MICROPHONES: OnceLock<Mutex<HashSet<usize>>> = OnceLock::new();

fn get_active_microphones() -> &'static Mutex<HashSet<usize>> {
    ACTIVE_MICROPHONES.get_or_init(|| Mutex::new(HashSet::new()))
}

/// xos.audio.get_input_devices() - Get all input (microphone) devices
pub fn get_input_devices(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let all_devices = audio::devices();
    
    // Filter to only input devices
    let input_devices: Vec<_> = all_devices
        .into_iter()
        .filter(|d| d.is_input)
        .collect();
    
    // Build device list by creating dicts manually
    let mut device_dicts = Vec::new();
    for (i, device) in input_devices.iter().enumerate() {
        let dict = vm.ctx.new_dict();
        dict.set_item("id", vm.ctx.new_int(i).into(), vm)?;
        dict.set_item("name", vm.ctx.new_str(device.name.clone()).into(), vm)?;
        device_dicts.push(dict.into());
    }
    
    // Create list from the dicts
    let list = vm.ctx.new_list(device_dicts);
    Ok(list.into())
}

/// xos.audio.Microphone(device_id=None, buffer_duration=1.0) - Create microphone instance
pub fn microphone_new(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    // Parse arguments - handle both positional and keyword args
    // device_id can be None (use default), or a specific device index
    let device_id_opt: Option<usize> = if !args.args.is_empty() {
        if args.args[0].is(&vm.ctx.none) {
            None
        } else {
            Some(args.args[0].clone().try_into_value::<usize>(vm)?)
        }
    } else if let Some(device_id_arg) = args.kwargs.get("device_id") {
        if device_id_arg.is(&vm.ctx.none) {
            None
        } else {
            Some(device_id_arg.clone().try_into_value::<usize>(vm)?)
        }
    } else {
        None // Default to None (use system default)
    };
    
    let buffer_duration = if args.args.len() > 1 {
        args.args[1].clone().try_into_value::<f64>(vm)? as f32
    } else if let Some(duration_arg) = args.kwargs.get("buffer_duration") {
        duration_arg.clone().try_into_value::<f64>(vm)? as f32
    } else {
        1.0
    };
    
    // Get the device to use
    let device = if let Some(device_id) = device_id_opt {
        // Specific device requested
        let all_devices = audio::devices();
        let input_devices: Vec<_> = all_devices
            .into_iter()
            .filter(|d| d.is_input)
            .collect();
        
        if input_devices.is_empty() {
            return Err(vm.new_runtime_error("No audio input devices (microphones) found".to_string()));
        }
        
        if device_id >= input_devices.len() {
            return Err(vm.new_runtime_error(format!("Invalid device_id: {}. Only {} device(s) available.", device_id, input_devices.len())));
        }
        
        input_devices[device_id].clone()
    } else {
        // Use default input device
        audio::default_input()
            .ok_or_else(|| vm.new_runtime_error("No default input device found".to_string()))?
    };
    
    let device = &device;
    
    // Create AudioListener
    let listener = audio::AudioListener::new(device, buffer_duration)
        .map_err(|e| vm.new_runtime_error(format!("Failed to initialize microphone: {}", e)))?;
    
    // Start recording
    listener.record()
        .map_err(|e| vm.new_runtime_error(format!("Failed to start recording: {}", e)))?;
    
    // Store the listener in a Box and get a raw pointer
    let listener_ptr = Box::into_raw(Box::new(listener)) as usize;
    
    // Register this microphone in the global registry
    if let Ok(mut mics) = get_active_microphones().lock() {
        mics.insert(listener_ptr);
    }
    
    // Create a Python class with pause/record methods for instant control
    let code = format!(r#"
class Microphone:
    def __init__(self, listener_ptr):
        self._listener_ptr = listener_ptr
    
    def get_batch(self, batch_size=1024):
        """
        Get a batch of audio samples.
        
        Args:
            batch_size: Number of samples to get (default: 1024)
            
        Returns:
            list: Batch of samples (mono channel, floats in range -1.0 to 1.0)
        """
        import xos
        return xos.audio._microphone_get_batch(self._listener_ptr, batch_size)
    
    def pause(self):
        """
        Pause microphone recording (mic light OFF instantly).
        """
        import xos
        return xos.audio._microphone_pause(self._listener_ptr)
    
    def record(self):
        """
        Resume microphone recording (mic light ON instantly).
        """
        import xos
        return xos.audio._microphone_record(self._listener_ptr)
    
    def batched_iterator(self, batch_size=1024):
        """
        Yields batches of audio samples.
        
        Args:
            batch_size: Number of samples per batch (default: 1024)
            
        Yields:
            list: Batch of samples (mono channel, floats in range -1.0 to 1.0)
        """
        import xos
        while True:
            batch = xos.audio._microphone_get_batch(self._listener_ptr, batch_size)
            if batch:
                yield batch
    
    def __del__(self):
        """Clean up the microphone when the object is destroyed."""
        if self._listener_ptr != 0:
            import xos
            xos.audio._microphone_cleanup(self._listener_ptr)
            self._listener_ptr = 0

_mic_instance = Microphone({})
"#, listener_ptr);
    
    let scope = vm.new_scope_with_builtins();
    vm.run_code_string(scope.clone(), &code, "<microphone>".to_string())?;
    
    // Get the instance from the scope
    let instance = scope.globals.get_item("_mic_instance", vm)?;
    Ok(instance)
}

/// Internal function to pause microphone recording (mic light OFF)
pub fn microphone_pause(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let listener_ptr: usize = args.bind(vm)?;
    
    if listener_ptr == 0 {
        return Err(vm.new_runtime_error("Invalid microphone pointer".to_string()));
    }
    
    // Convert pointer back to reference
    let listener = unsafe { &*(listener_ptr as *const audio::AudioListener) };
    
    // Pause recording (mic light OFF)
    listener.pause()
        .map_err(|e| vm.new_runtime_error(format!("Failed to pause microphone: {}", e)))?;
    
    Ok(vm.ctx.none())
}

/// Internal function to resume microphone recording (mic light ON)
pub fn microphone_record(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let listener_ptr: usize = args.bind(vm)?;
    
    if listener_ptr == 0 {
        return Err(vm.new_runtime_error("Invalid microphone pointer".to_string()));
    }
    
    // Convert pointer back to reference
    let listener = unsafe { &*(listener_ptr as *const audio::AudioListener) };
    
    // Resume recording (mic light ON)
    listener.record()
        .map_err(|e| vm.new_runtime_error(format!("Failed to resume microphone: {}", e)))?;
    
    Ok(vm.ctx.none())
}

/// Internal function to get a batch of samples from the microphone
pub fn microphone_get_batch(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let (listener_ptr, batch_size): (usize, usize) = args.bind(vm)?;
    
    if listener_ptr == 0 {
        return Err(vm.new_runtime_error("Invalid microphone pointer".to_string()));
    }
    
    // Convert pointer back to reference
    let listener = unsafe { &*(listener_ptr as *const audio::AudioListener) };
    
    // Get samples from all channels
    let all_samples = listener.get_samples_by_channel();
    
    if all_samples.is_empty() {
        // Return empty array
        let dict = vm.ctx.new_dict();
        dict.set_item("_data", vm.ctx.new_list(vec![]).into(), vm)?;
        dict.set_item("shape", vm.ctx.new_tuple(vec![vm.ctx.new_int(0).into()]).into(), vm)?;
        dict.set_item("dtype", vm.ctx.new_str("float32").into(), vm)?;
        return Ok(dict.into());
    }
    
    // Use the first channel (mono)
    let samples = &all_samples[0];
    
    // Take up to batch_size samples and convert to Python list
    let batch: Vec<PyObjectRef> = samples.iter()
        .take(batch_size)
        .map(|&s| vm.ctx.new_float(s as f64).into())
        .collect();
    
    let py_list = vm.ctx.new_list(batch);
    
    // CRITICAL: Clear the listener buffer after reading to avoid re-queueing
    // the same samples on the next call! (just like the Rust audio_relay.rs app does)
    listener.buffer().clear();
    
    // Create xos.Array dict
    let dict = vm.ctx.new_dict();
    dict.set_item("_data", py_list.into(), vm)?;
    dict.set_item("shape", vm.ctx.new_tuple(vec![vm.ctx.new_int(samples.len().min(batch_size) as i32).into()]).into(), vm)?;
    dict.set_item("dtype", vm.ctx.new_str("float32").into(), vm)?;
    
    // Wrap in _ArrayWrapper for nice display
    if let Ok(wrapper_class) = vm.builtins.get_attr("_ArrayWrapper", vm) {
        if let Ok(wrapped) = wrapper_class.call((dict.clone(),), vm) {
            return Ok(wrapped);
        }
    }
    
    Ok(dict.into())
}

/// Internal function to clean up a microphone (drop the AudioListener)
pub fn microphone_cleanup(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let listener_ptr: usize = args.bind(vm)?;
    
    if listener_ptr == 0 {
        return Ok(vm.ctx.none());
    }
    
    // Check if this pointer is still in the registry
    // If it's not, it was already cleaned up by cleanup_all_audio() - don't double-free!
    let was_in_registry = if let Ok(mut mics) = get_active_microphones().lock() {
        mics.remove(&listener_ptr)
    } else {
        false
    };
    
    if !was_in_registry {
        // Already cleaned up by cleanup_all_audio() - skip to avoid double-free
        return Ok(vm.ctx.none());
    }
    
    // Immediately destroy at iOS level
    #[cfg(target_os = "ios")]
    unsafe {
        let listener = &*(listener_ptr as *const audio::AudioListener);
        listener.destroy_now(); // Instant destroy!
    }
    
    // Then drop Rust-side object (won't double-destroy due to flag)
    unsafe {
        let _ = Box::from_raw(listener_ptr as *mut audio::AudioListener);
    }
    
    Ok(vm.ctx.none())
}

/// Clean up ALL active microphones (called when stopping app or switching)
pub fn cleanup_all_microphones(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    cleanup_all_microphones_rust();
    Ok(vm.ctx.none())
}

/// Rust-side function to cleanup all microphones (called from CoderApp Drop)
pub fn cleanup_all_microphones_rust() {
    let mic_ptrs: Vec<usize> = if let Ok(mut mics) = get_active_microphones().lock() {
        let ptrs: Vec<usize> = mics.drain().collect();
        ptrs
    } else {
        vec![]
    };
    
    // Immediately destroy at iOS level (instant mic light off)
    #[cfg(target_os = "ios")]
    {
        for &ptr in &mic_ptrs {
            if ptr != 0 {
                unsafe {
                    let listener = &*(ptr as *const audio::AudioListener);
                    listener.destroy_now(); // Instant destroy!
                }
            }
        }
    }
    
    // Then drop the Rust-side objects (iOS cleanup is already done, won't double-destroy)
    for ptr in mic_ptrs {
        if ptr != 0 {
            unsafe {
                let _ = Box::from_raw(ptr as *mut audio::AudioListener);
            }
        }
    }
}

