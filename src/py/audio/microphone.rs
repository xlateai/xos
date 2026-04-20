use rustpython_vm::{AsObject, PyResult, VirtualMachine, function::FuncArgs, PyObjectRef};
use crate::engine::audio;
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
        dict.set_item(
            "label",
            vm.ctx.new_str(device.input_menu_label()).into(),
            vm)?;
        device_dicts.push(dict.into());
    }
    
    // Create list from the dicts
    let list = vm.ctx.new_list(device_dicts);
    Ok(list.into())
}

fn parse_mic_buffer_duration(args: &FuncArgs, vm: &VirtualMachine) -> PyResult<f32> {
    if args.args.len() > 1 {
        Ok(args.args[1].clone().try_into_value::<f64>(vm)? as f32)
    } else if let Some(duration_arg) = args
        .kwargs
        .get("buffer_duration")
        .or_else(|| args.kwargs.get("max_buffer_duration"))
    {
        Ok(duration_arg.clone().try_into_value::<f64>(vm)? as f32)
    } else {
        Ok(1.0)
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios"), any(target_os = "macos", target_os = "windows")))]
#[allow(dead_code)]
fn parse_system_buffer_duration(args: &FuncArgs, vm: &VirtualMachine) -> PyResult<f32> {
    if !args.args.is_empty() {
        Ok(args.args[0].clone().try_into_value::<f64>(vm)? as f32)
    } else if let Some(d) = args
        .kwargs
        .get("buffer_duration")
        .or_else(|| args.kwargs.get("max_buffer_duration"))
    {
        Ok(d.clone().try_into_value::<f64>(vm)? as f32)
    } else {
        Ok(10.0)
    }
}

/// Build `Microphone` Python object from a resolved [`audio::AudioDevice`].
pub fn microphone_from_resolved_device(device: &audio::AudioDevice, buffer_duration: f32, vm: &VirtualMachine) -> PyResult {
    let listener = audio::AudioListener::new(device, buffer_duration)
        .map_err(|e| vm.new_runtime_error(format!("Failed to initialize microphone: {e}")))?;
    listener
        .record()
        .map_err(|e| vm.new_runtime_error(format!("Failed to start recording: {e}")))?;
    let listener_ptr = Box::into_raw(Box::new(listener)) as usize;
    if let Ok(mut mics) = get_active_microphones().lock() {
        mics.insert(listener_ptr);
    }
    install_microphone_python_wrapper(listener_ptr, vm)
}

/// xos.audio.Microphone(device_id=None, buffer_duration=1.0) - Create microphone instance
pub fn microphone_new(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
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
        None
    };

    let buffer_duration = parse_mic_buffer_duration(&args, vm)?;

    let device = if let Some(device_id) = device_id_opt {
        let all_devices = audio::devices();
        let input_devices: Vec<_> = all_devices.into_iter().filter(|d| d.is_input).collect();

        if input_devices.is_empty() {
            return Err(vm.new_runtime_error(
                "No audio input devices found (on Windows, expect “… (system audio)” entries for each output)"
                    .to_string(),
            ));
        }

        if device_id >= input_devices.len() {
            return Err(vm.new_runtime_error(format!(
                "Invalid device_id: {}. Only {} device(s) available.",
                device_id,
                input_devices.len()
            )));
        }

        input_devices[device_id].clone()
    } else {
        audio::default_input()
            .ok_or_else(|| vm.new_runtime_error("No default input device found".to_string()))?
    };

    microphone_from_resolved_device(&device, buffer_duration, vm)
}

/// xos.audio.system(buffer_duration=10.0) — prefer system / loopback input (macOS ScreenCaptureKit, Windows loopback, virtual cables).
pub fn microphone_system(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    #[cfg(any(target_arch = "wasm32", target_os = "ios"))]
    {
        let _ = args;
        return Err(vm.new_runtime_error(
            "xos.audio.system is only available on desktop (macOS / Linux / Windows)".to_string(),
        ));
    }
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios"), any(target_os = "macos", target_os = "windows")))]
    {
        let buffer_duration = parse_system_buffer_duration(&args, vm)?;
        let device = audio::preferred_system_audio_input_device().ok_or_else(|| {
            vm.new_runtime_error(
                "No system or loopback input found. On macOS use “System audio”; on Windows use a “(system audio)” capture device; or install a virtual cable (e.g. BlackHole)."
                    .to_string(),
            )
        })?;
        microphone_from_resolved_device(&device, buffer_duration, vm)
    }

    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios"), target_os = "linux"))]
    {
        let _ = args;
        Err(vm.new_runtime_error(
            "xos.audio.system is unavailable on this Linux no-audio build".to_string(),
        ))
    }
}

fn install_microphone_python_wrapper(listener_ptr: usize, vm: &VirtualMachine) -> PyResult {
    let code = format!(r#"
class Microphone:
    '''Shared rolling capture ring for this device.

    ``buffer_duration`` (alias ``max_buffer_duration``) is the **maximum** seconds of audio
    retained per channel; older samples drop when the ring is full. Waveforms and transcription
    **peek**; ``read()`` **drains**. MP3 recording reads incrementally without draining the ring.
    '''

    def __init__(self, listener_ptr):
        self._listener_ptr = listener_ptr
    
    def get_batch(self, batch_size=None):
        """
        Get a batch of audio samples WITHOUT removing them from the buffer.
        
        This is a 'peek' operation - samples remain in the buffer for visualization.
        Use this for waveform displays where you want smooth continuous data.
        
        Args:
            batch_size: Number of samples to get. If None (default), gets ALL samples.
            
        Returns:
            xos.Tensor: Batch of samples (mono channel, floats in range -1.0 to 1.0)
        """
        import xos
        if batch_size is None:
            # Get ALL samples (default behavior - matches Rust get_samples_by_channel())
            return xos.audio._microphone_get_all(self._listener_ptr)
        else:
            return xos.audio._microphone_get_batch(self._listener_ptr, batch_size)
    
    def read(self, batch_size=None):
        """
        Read (drain) audio samples, removing them from the buffer.
        
        This is a 'consume' operation - samples are removed after reading.
        Use this for audio relay where you don't want to repeat samples.
        
        Args:
            batch_size: Number of samples to read. If None, reads ALL available samples.
            
        Returns:
            xos.Tensor: Batch of samples (mono channel, floats in range -1.0 to 1.0)
        """
        import xos
        if batch_size is None:
            # Read ALL available samples (like Rust code)
            samples = xos.audio._microphone_read_all(self._listener_ptr)
        else:
            samples = xos.audio._microphone_read_batch(self._listener_ptr, batch_size)
        
        # Auto-clear after reading to prevent re-processing
        xos.audio._microphone_clear(self._listener_ptr)
        return samples
    
    def clear(self):
        """
        Clear (empty) the microphone buffer.
        
        Use after reading samples in audio relay to prevent re-processing.
        This is the same as Rust's listener.buffer().clear() pattern.
        """
        import xos
        return xos.audio._microphone_clear(self._listener_ptr)
    
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
    
    def get_sample_rate(self):
        """
        Get the microphone's actual sample rate.
        
        Returns:
            int: Sample rate in Hz (e.g., 16000, 44100, 48000)
        """
        import xos
        return xos.audio._microphone_get_sample_rate(self._listener_ptr)
    
    def batched_iterator(self, batch_size=1024):
        """
        Yields batches of audio samples (consuming mode).
        
        Args:
            batch_size: Number of samples per batch (default: 1024)
            
        Yields:
            xos.Tensor: Batch of samples (mono channel, floats in range -1.0 to 1.0)
        """
        import xos
        while True:
            batch = xos.audio._microphone_read_batch(self._listener_ptr, batch_size)
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

/// Internal function to get the microphone's sample rate
pub fn microphone_get_sample_rate(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let listener_ptr: usize = args.bind(vm)?;
    
    if listener_ptr == 0 {
        return Err(vm.new_runtime_error("Invalid microphone pointer".to_string()));
    }
    
    // Convert pointer back to reference
    let listener = unsafe { &*(listener_ptr as *const audio::AudioListener) };
    
    // Get the sample rate
    let sample_rate = listener.buffer().sample_rate();
    
    Ok(vm.ctx.new_int(sample_rate).into())
}

/// Internal function to get ALL samples from the microphone (peek mode - does NOT drain)
/// This matches Rust's get_samples_by_channel() - returns everything in the buffer
pub fn microphone_get_all(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let listener_ptr: usize = args.bind(vm)?;
    
    if listener_ptr == 0 {
        return Err(vm.new_runtime_error("Invalid microphone pointer".to_string()));
    }
    
    // Convert pointer back to reference
    let listener = unsafe { &*(listener_ptr as *const audio::AudioListener) };
    
    // Get ALL samples from all channels (peek - does not drain)
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
    
    // Convert all samples to Python list
    let batch: Vec<PyObjectRef> = samples.iter()
        .map(|&s| vm.ctx.new_float(s as f64).into())
        .collect();
    
    let py_list = vm.ctx.new_list(batch);
    
    // Create xos.Tensor dict
    let dict = vm.ctx.new_dict();
    dict.set_item("_data", py_list.into(), vm)?;
    dict.set_item("shape", vm.ctx.new_tuple(vec![vm.ctx.new_int(samples.len() as i32).into()]).into(), vm)?;
    dict.set_item("dtype", vm.ctx.new_str("float32").into(), vm)?;
    
    // Wrap in _TensorWrapper for nice display
    if let Ok(wrapper_class) = vm.builtins.get_attr("Tensor", vm) {
        if let Ok(wrapped) = wrapper_class.call((dict.clone(),), vm) {
            return Ok(wrapped);
        }
    }
    
    Ok(dict.into())
}

/// Internal function to get a batch of samples from the microphone (peek mode - does NOT drain)
pub fn microphone_get_batch(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let (listener_ptr, batch_size): (usize, usize) = args.bind(vm)?;
    
    if listener_ptr == 0 {
        return Err(vm.new_runtime_error("Invalid microphone pointer".to_string()));
    }
    
    // Convert pointer back to reference
    let listener = unsafe { &*(listener_ptr as *const audio::AudioListener) };
    
    // Get samples from all channels (peek - does not drain)
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
    let actual_size = samples.len().min(batch_size);
    let batch: Vec<PyObjectRef> = samples.iter()
        .take(actual_size)
        .map(|&s| vm.ctx.new_float(s as f64).into())
        .collect();
    
    let py_list = vm.ctx.new_list(batch);
    
    // NOTE: We do NOT drain the buffer here! This is peek mode for visualization.
    
    // Create xos.Tensor dict
    let dict = vm.ctx.new_dict();
    dict.set_item("_data", py_list.into(), vm)?;
    dict.set_item("shape", vm.ctx.new_tuple(vec![vm.ctx.new_int(actual_size as i32).into()]).into(), vm)?;
    dict.set_item("dtype", vm.ctx.new_str("float32").into(), vm)?;
    
    // Wrap in _TensorWrapper for nice display
    if let Ok(wrapper_class) = vm.builtins.get_attr("Tensor", vm) {
        if let Ok(wrapped) = wrapper_class.call((dict.clone(),), vm) {
            return Ok(wrapped);
        }
    }
    
    Ok(dict.into())
}

/// Internal function to read (drain) ALL samples from the microphone
/// This matches Rust audio_relay.rs behavior - get everything available
pub fn microphone_read_all(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let listener_ptr: usize = args.bind(vm)?;
    
    if listener_ptr == 0 {
        return Err(vm.new_runtime_error("Invalid microphone pointer".to_string()));
    }
    
    // Convert pointer back to reference
    let listener = unsafe { &*(listener_ptr as *const audio::AudioListener) };
    
    // Get current buffer size to drain everything
    let buffer_len = listener.buffer().len();
    
    // Drain ALL samples from the buffer (FIFO consume operation)
    let all_samples = listener.buffer().drain_samples(buffer_len);
    
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
    
    // Convert all drained samples to Python list
    let batch: Vec<PyObjectRef> = samples.iter()
        .map(|&s| vm.ctx.new_float(s as f64).into())
        .collect();
    
    let py_list = vm.ctx.new_list(batch);
    
    // Create xos.Tensor dict
    let dict = vm.ctx.new_dict();
    dict.set_item("_data", py_list.into(), vm)?;
    dict.set_item("shape", vm.ctx.new_tuple(vec![vm.ctx.new_int(samples.len() as i32).into()]).into(), vm)?;
    dict.set_item("dtype", vm.ctx.new_str("float32").into(), vm)?;
    
    // Wrap in _TensorWrapper for nice display
    if let Ok(wrapper_class) = vm.builtins.get_attr("Tensor", vm) {
        if let Ok(wrapped) = wrapper_class.call((dict.clone(),), vm) {
            return Ok(wrapped);
        }
    }
    
    Ok(dict.into())
}

/// Internal function to read (drain) a batch of samples from the microphone
/// This REMOVES samples from the buffer - use for audio relay to prevent repeats
pub fn microphone_read_batch(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let (listener_ptr, batch_size): (usize, usize) = args.bind(vm)?;
    
    if listener_ptr == 0 {
        return Err(vm.new_runtime_error("Invalid microphone pointer".to_string()));
    }
    
    // Convert pointer back to reference
    let listener = unsafe { &*(listener_ptr as *const audio::AudioListener) };
    
    // Drain samples from the buffer (FIFO consume operation)
    let all_samples = listener.buffer().drain_samples(batch_size);
    
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
    
    // Convert all drained samples to Python list
    let batch: Vec<PyObjectRef> = samples.iter()
        .map(|&s| vm.ctx.new_float(s as f64).into())
        .collect();
    
    let py_list = vm.ctx.new_list(batch);
    
    // Create xos.Tensor dict
    let dict = vm.ctx.new_dict();
    dict.set_item("_data", py_list.into(), vm)?;
    dict.set_item("shape", vm.ctx.new_tuple(vec![vm.ctx.new_int(samples.len() as i32).into()]).into(), vm)?;
    dict.set_item("dtype", vm.ctx.new_str("float32").into(), vm)?;
    
    // Wrap in _TensorWrapper for nice display
    if let Ok(wrapper_class) = vm.builtins.get_attr("Tensor", vm) {
        if let Ok(wrapped) = wrapper_class.call((dict.clone(),), vm) {
            return Ok(wrapped);
        }
    }
    
    Ok(dict.into())
}

/// Internal function to clear the microphone buffer
pub fn microphone_clear(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let listener_ptr: usize = args.bind(vm)?;
    
    if listener_ptr == 0 {
        return Err(vm.new_runtime_error("Invalid microphone pointer".to_string()));
    }
    
    // Convert pointer back to reference
    let listener = unsafe { &*(listener_ptr as *const audio::AudioListener) };
    
    // Clear the buffer
    listener.buffer().clear();
    
    Ok(vm.ctx.none())
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

