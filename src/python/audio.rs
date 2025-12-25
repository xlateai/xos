use rustpython_vm::{PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs, PyObjectRef};
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
fn get_input_devices(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
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

/// xos.audio.Microphone(device_id=0, buffer_duration=1.0) - Create microphone instance
fn microphone_new(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    // Parse arguments - handle both positional and keyword args
    let device_id = if !args.args.is_empty() {
        args.args[0].clone().try_into_value::<usize>(vm)?
    } else if let Some(device_id_arg) = args.kwargs.get("device_id") {
        device_id_arg.clone().try_into_value::<usize>(vm)?
    } else {
        0
    };
    
    let buffer_duration = if args.args.len() > 1 {
        args.args[1].clone().try_into_value::<f64>(vm)? as f32
    } else if let Some(duration_arg) = args.kwargs.get("buffer_duration") {
        duration_arg.clone().try_into_value::<f64>(vm)? as f32
    } else {
        1.0
    };
    
    // Get all input devices
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
    
    let device = &input_devices[device_id];
    
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
    
    // Create a Python class with batched_iterator method and cleanup
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

/// Internal function to get a batch of samples from the microphone
fn microphone_get_batch(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
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
fn microphone_cleanup(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let listener_ptr: usize = args.bind(vm)?;
    
    if listener_ptr == 0 {
        return Ok(vm.ctx.none());
    }
    
    // Remove from registry
    if let Ok(mut mics) = get_active_microphones().lock() {
        mics.remove(&listener_ptr);
    }
    
    // Convert pointer back to Box and drop it
    unsafe {
        let _listener = Box::from_raw(listener_ptr as *mut audio::AudioListener);
        // _listener is automatically dropped here, which stops recording
    }
    
    Ok(vm.ctx.none())
}

/// Clean up ALL active microphones (called when stopping app or switching)
fn cleanup_all_microphones(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let mic_ptrs: Vec<usize> = if let Ok(mut mics) = get_active_microphones().lock() {
        let ptrs: Vec<usize> = mics.drain().collect();
        ptrs
    } else {
        vec![]
    };
    
    // Drop all microphones
    for ptr in mic_ptrs {
        if ptr != 0 {
            unsafe {
                let _listener = Box::from_raw(ptr as *mut audio::AudioListener);
                // _listener is automatically dropped here, which stops recording
            }
        }
    }
    
    Ok(vm.ctx.none())
}

/// Create the audio module
pub fn make_audio_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.audio", vm.ctx.new_dict(), None);
    
    // Public API
    module.set_attr("get_input_devices", vm.new_function("get_input_devices", get_input_devices), vm).unwrap();
    module.set_attr("Microphone", vm.new_function("Microphone", microphone_new), vm).unwrap();
    module.set_attr("cleanup_all_microphones", vm.new_function("cleanup_all_microphones", cleanup_all_microphones), vm).unwrap();
    
    // Internal functions
    module.set_attr("_microphone_get_batch", vm.new_function("_microphone_get_batch", microphone_get_batch), vm).unwrap();
    module.set_attr("_microphone_cleanup", vm.new_function("_microphone_cleanup", microphone_cleanup), vm).unwrap();
    
    module
}

/// Rust-side function to cleanup all microphones (called from CoderApp Drop)
pub fn cleanup_all_microphones_rust() {
    let mic_ptrs: Vec<usize> = if let Ok(mut mics) = get_active_microphones().lock() {
        let ptrs: Vec<usize> = mics.drain().collect();
        ptrs
    } else {
        vec![]
    };
    
    // Drop all microphones
    for ptr in mic_ptrs {
        if ptr != 0 {
            unsafe {
                let _listener = Box::from_raw(ptr as *mut audio::AudioListener);
                // _listener is automatically dropped here, which stops recording
            }
        }
    }
}

