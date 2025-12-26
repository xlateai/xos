use rustpython_vm::{PyResult, VirtualMachine, function::FuncArgs, PyObjectRef, AsObject};
use crate::audio;
use std::sync::Mutex;
use std::collections::HashSet;
use std::sync::OnceLock;

// Global registry to track all active speaker pointers
static ACTIVE_SPEAKERS: OnceLock<Mutex<HashSet<usize>>> = OnceLock::new();

fn get_active_speakers() -> &'static Mutex<HashSet<usize>> {
    ACTIVE_SPEAKERS.get_or_init(|| Mutex::new(HashSet::new()))
}

// --- iOS AudioPlayer (uses Swift FFI) ---
#[cfg(target_os = "ios")]
pub struct AudioPlayer {
    player_id: u32,
    sample_rate: u32,
    channels: u16,
}

#[cfg(target_os = "ios")]
impl AudioPlayer {
    pub fn new(device_id: u32, sample_rate: u32, channels: u16) -> Result<Self, String> {
        let player_id = unsafe {
            xos_audio_player_init(device_id, sample_rate as f64, channels as u32)
        };
        
        if player_id == u32::MAX {
            return Err("Failed to initialize audio player".to_string());
        }
        
        Ok(Self {
            player_id,
            sample_rate,
            channels,
        })
    }
    
    pub fn play_samples(&self, samples: &[f32]) -> Result<(), String> {
        let result = unsafe {
            xos_audio_player_queue_samples(
                self.player_id,
                samples.as_ptr(),
                samples.len(),
            )
        };
        
        if result != 0 {
            return Err("Failed to queue audio samples".to_string());
        }
        
        Ok(())
    }
    
    pub fn get_buffer_size(&self) -> usize {
        unsafe { xos_audio_player_get_buffer_size(self.player_id) as usize }
    }
    
    pub fn start(&self) -> Result<(), String> {
        let result = unsafe { xos_audio_player_start(self.player_id) };
        if result != 0 {
            return Err("Failed to start audio player".to_string());
        }
        Ok(())
    }
    
    #[allow(dead_code)]
    pub fn stop(&self) -> Result<(), String> {
        let result = unsafe { xos_audio_player_stop(self.player_id) };
        if result != 0 {
            return Err("Failed to stop audio player".to_string());
        }
        Ok(())
    }
}

#[cfg(target_os = "ios")]
impl Drop for AudioPlayer {
    fn drop(&mut self) {
        unsafe {
            xos_audio_player_destroy(self.player_id);
        }
    }
}

// --- Native AudioPlayer (uses cpal via audio crate) ---
#[cfg(all(not(target_os = "ios"), not(target_arch = "wasm32")))]
pub struct AudioPlayer {
    inner: audio::AudioPlayer,
}

#[cfg(all(not(target_os = "ios"), not(target_arch = "wasm32")))]
impl AudioPlayer {
    pub fn play_samples(&self, samples: &[f32]) -> Result<(), String> {
        self.inner.play_samples(samples)
    }
    
    pub fn get_buffer_size(&self) -> usize {
        self.inner.get_buffer_size()
    }
    
    pub fn start(&self) -> Result<(), String> {
        self.inner.start()
    }
    
    #[allow(dead_code)]
    pub fn stop(&self) -> Result<(), String> {
        self.inner.stop()
    }
}

// --- WASM stub (not implemented yet) ---
#[cfg(target_arch = "wasm32")]
pub struct AudioPlayer;

#[cfg(target_arch = "wasm32")]
impl AudioPlayer {
    pub fn new(_device_id: u32, _sample_rate: u32, _channels: u16) -> Result<Self, String> {
        Err("Audio player not yet available on WASM".to_string())
    }
    
    pub fn play_samples(&self, _samples: &[f32]) -> Result<(), String> {
        Err("Audio player not yet available on WASM".to_string())
    }
    
    pub fn get_buffer_size(&self) -> usize {
        0
    }
    
    pub fn start(&self) -> Result<(), String> {
        Err("Audio player not yet available on WASM".to_string())
    }
    
    #[allow(dead_code)]
    pub fn stop(&self) -> Result<(), String> {
        Err("Audio player not yet available on WASM".to_string())
    }
}

/// xos.audio.get_output_devices() - Get all output (speaker) devices
pub fn get_output_devices(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let all_devices = audio::devices();
    
    // Filter to only output devices
    let output_devices: Vec<_> = all_devices
        .into_iter()
        .filter(|d| d.is_output)
        .collect();
    
    // Build device list by creating dicts manually
    let mut device_dicts = Vec::new();
    for (i, device) in output_devices.iter().enumerate() {
        let dict = vm.ctx.new_dict();
        dict.set_item("id", vm.ctx.new_int(i).into(), vm)?;
        dict.set_item("name", vm.ctx.new_str(device.name.clone()).into(), vm)?;
        device_dicts.push(dict.into());
    }
    
    // Create list from the dicts
    let list = vm.ctx.new_list(device_dicts);
    Ok(list.into())
}

/// xos.audio.Speaker(device_id=None, sample_rate=44100, channels=1) - Create speaker instance
pub fn speaker_new(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    // Parse arguments
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
    
    let sample_rate = if args.args.len() > 1 {
        args.args[1].clone().try_into_value::<usize>(vm)?
    } else if let Some(rate_arg) = args.kwargs.get("sample_rate") {
        rate_arg.clone().try_into_value::<usize>(vm)?
    } else {
        44100
    };
    
    let channels = if args.args.len() > 2 {
        args.args[2].clone().try_into_value::<usize>(vm)?
    } else if let Some(channels_arg) = args.kwargs.get("channels") {
        channels_arg.clone().try_into_value::<usize>(vm)?
    } else {
        1
    };
    
        // Get the device to use and create AudioPlayer
        let player = if let Some(device_id) = device_id_opt {
            // Specific device requested
            let all_devices = audio::devices();
            let output_devices: Vec<_> = all_devices
                .into_iter()
                .filter(|d| d.is_output)
                .collect();
            
            if output_devices.is_empty() {
                return Err(vm.new_runtime_error("No audio output devices (speakers) found".to_string()));
            }
            
            if device_id >= output_devices.len() {
                return Err(vm.new_runtime_error(format!("Invalid device_id: {}. Only {} device(s) available.", device_id, output_devices.len())));
            }
            
            #[cfg(target_os = "ios")]
            {
                let actual_device_id = output_devices[device_id].device_id;
                AudioPlayer::new(actual_device_id, sample_rate as u32, channels as u16)
                    .map_err(|e| vm.new_runtime_error(format!("Failed to initialize speaker: {}", e)))?
            }
            
            #[cfg(not(target_os = "ios"))]
            {
                // On native, create directly with the device
                let device = &output_devices[device_id];
                let inner = audio::AudioPlayer::new(device, sample_rate as u32, channels as u16)
                    .map_err(|e| vm.new_runtime_error(format!("Failed to initialize speaker: {}", e)))?;
                AudioPlayer { inner }
            }
        } else {
            // Use default output device
            let default_device = audio::default_output()
                .ok_or_else(|| vm.new_runtime_error("No default output device found".to_string()))?;
            
            #[cfg(target_os = "ios")]
            {
                AudioPlayer::new(default_device.device_id, sample_rate as u32, channels as u16)
                    .map_err(|e| vm.new_runtime_error(format!("Failed to initialize speaker: {}", e)))?
            }
            
            #[cfg(not(target_os = "ios"))]
            {
                // On native, create directly with the default device
                let inner = audio::AudioPlayer::new(&default_device, sample_rate as u32, channels as u16)
                    .map_err(|e| vm.new_runtime_error(format!("Failed to initialize speaker: {}", e)))?;
                AudioPlayer { inner }
            }
        };
    
    // Start playback
    player.start()
        .map_err(|e| vm.new_runtime_error(format!("Failed to start speaker: {}", e)))?;
    
    // Store the player in a Box and get a raw pointer
    let player_ptr = Box::into_raw(Box::new(player)) as usize;
    
    // Register this speaker in the global registry
    if let Ok(mut speakers) = get_active_speakers().lock() {
        speakers.insert(player_ptr);
    }
    
    // Create a Python class for the Speaker
    let code = format!(r#"
class Speaker:
    def __init__(self, player_ptr):
        self._player_ptr = player_ptr
        self._sample_rate = {}
        self._channels = {}
    
    def play_sample_batch(self, samples):
        """
        Queue audio samples for playback.
        
        Args:
            samples: xos.Array of audio samples (floats in range -1.0 to 1.0)
                     Shape: (time_steps,) for mono or (time_steps, channels) for multi-channel
        """
        import xos
        return xos.audio._speaker_play_batch(self._player_ptr, samples)
    
    @property
    def samples_buffer(self):
        """
        Get a view of the current unplayed samples buffer.
        
        Returns:
            xos.Array: Current buffer state with shape (buffer_size, channels)
        """
        import xos
        return xos.audio._speaker_get_buffer(self._player_ptr)
    
    @property
    def sample_rate(self):
        """Get the sample rate of the speaker."""
        return self._sample_rate
    
    @property
    def channels(self):
        """Get the number of channels."""
        return self._channels
    
    def __del__(self):
        """Clean up the speaker when the object is destroyed."""
        if self._player_ptr != 0:
            import xos
            xos.audio._speaker_cleanup(self._player_ptr)
            self._player_ptr = 0

_speaker_instance = Speaker({})
"#, sample_rate, channels, player_ptr);
    
    let scope = vm.new_scope_with_builtins();
    vm.run_code_string(scope.clone(), &code, "<speaker>".to_string())?;
    
    // Get the instance from the scope
    let instance = scope.globals.get_item("_speaker_instance", vm)?;
    Ok(instance)
}

/// Internal function to play a batch of samples through the speaker
pub fn speaker_play_batch(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let (player_ptr, samples_obj): (usize, PyObjectRef) = args.bind(vm)?;
    
    if player_ptr == 0 {
        return Err(vm.new_runtime_error("Invalid speaker pointer".to_string()));
    }
    
    // Convert pointer back to reference
    let player = unsafe { &*(player_ptr as *const AudioPlayer) };
    
    // Extract samples from the array-like object
    let samples = extract_samples_from_array(samples_obj, vm)?;
    
    // Play the samples
    player.play_samples(&samples)
        .map_err(|e| vm.new_runtime_error(format!("Failed to play samples: {}", e)))?;
    
    Ok(vm.ctx.none())
}

/// Internal function to get the current buffer state
pub fn speaker_get_buffer(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let player_ptr: usize = args.bind(vm)?;
    
    if player_ptr == 0 {
        return Err(vm.new_runtime_error("Invalid speaker pointer".to_string()));
    }
    
    // Convert pointer back to reference
    let player = unsafe { &*(player_ptr as *const AudioPlayer) };
    
    // Get buffer size
    let buffer_size = player.get_buffer_size();
    
    // Create a simple array dict representing the buffer state
    let dict = vm.ctx.new_dict();
    dict.set_item("_data", vm.ctx.new_list(vec![]).into(), vm)?;
    dict.set_item("shape", vm.ctx.new_tuple(vec![vm.ctx.new_int(buffer_size as i32).into()]).into(), vm)?;
    dict.set_item("dtype", vm.ctx.new_str("float32").into(), vm)?;
    
    // Wrap in _ArrayWrapper for nice display
    if let Ok(wrapper_class) = vm.builtins.get_attr("_ArrayWrapper", vm) {
        if let Ok(wrapped) = wrapper_class.call((dict.clone(),), vm) {
            return Ok(wrapped);
        }
    }
    
    Ok(dict.into())
}

/// Internal function to clean up a speaker (drop the AudioPlayer)
pub fn speaker_cleanup(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let player_ptr: usize = args.bind(vm)?;
    
    if player_ptr == 0 {
        return Ok(vm.ctx.none());
    }
    
    // Check if this pointer is still in the registry
    // If it's not, it was already cleaned up by cleanup_all_audio() - don't double-free!
    let was_in_registry = if let Ok(mut speakers) = get_active_speakers().lock() {
        speakers.remove(&player_ptr)
    } else {
        false
    };
    
    if !was_in_registry {
        // Already cleaned up by cleanup_all_audio() - skip to avoid double-free
        return Ok(vm.ctx.none());
    }
    
    // Drop Rust-side object
    unsafe {
        let _ = Box::from_raw(player_ptr as *mut AudioPlayer);
    }
    
    Ok(vm.ctx.none())
}

/// Clean up ALL active speakers (called when stopping app or switching)
pub fn cleanup_all_speakers(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    cleanup_all_speakers_rust();
    Ok(vm.ctx.none())
}

/// Rust-side function to cleanup all speakers (called from CoderApp Drop or cleanup_all_audio)
pub fn cleanup_all_speakers_rust() {
    let speaker_ptrs: Vec<usize> = if let Ok(mut speakers) = get_active_speakers().lock() {
        let ptrs: Vec<usize> = speakers.drain().collect();
        ptrs
    } else {
        vec![]
    };
    
    // Drop the Rust-side objects
    for ptr in speaker_ptrs {
        if ptr != 0 {
            unsafe {
                let _ = Box::from_raw(ptr as *mut AudioPlayer);
            }
        }
    }
}

/// Helper function to extract f32 samples from a Python array-like object
fn extract_samples_from_array(obj: PyObjectRef, vm: &VirtualMachine) -> Result<Vec<f32>, rustpython_vm::builtins::PyBaseExceptionRef> {
    // Helper to convert a Python object to f32 (handles both int and float)
    fn to_f32(item: PyObjectRef, vm: &VirtualMachine) -> Result<f32, rustpython_vm::builtins::PyBaseExceptionRef> {
        // Try as float first
        if let Ok(value) = item.clone().try_into_value::<f64>(vm) {
            return Ok(value as f32);
        }
        // Try as int
        if let Ok(value) = item.clone().try_into_value::<i64>(vm) {
            return Ok(value as f32);
        }
        Err(vm.new_type_error(format!("Expected numeric type, got {:?}", item.class().name())))
    }
    
    // Try to get _data attribute (xos.Array format)
    if let Ok(data_attr) = obj.get_attr("_data", vm) {
        // It's an xos.Array, extract the _data list
        if let Ok(list) = data_attr.downcast::<rustpython_vm::builtins::PyList>() {
            let mut samples = Vec::new();
            for item in list.borrow_vec().iter() {
                samples.push(to_f32(item.clone(), vm)?);
            }
            return Ok(samples);
        }
    }
    
    // Try as a direct list
    if let Ok(list) = obj.downcast::<rustpython_vm::builtins::PyList>() {
        let mut samples = Vec::new();
        for item in list.borrow_vec().iter() {
            samples.push(to_f32(item.clone(), vm)?);
        }
        return Ok(samples);
    }
    
    Err(vm.new_type_error("Expected array or list of samples".to_string()))
}

// FFI declarations for iOS audio player functions
#[cfg(target_os = "ios")]
extern "C" {
    fn xos_audio_player_init(
        device_id: u32,
        sample_rate: f64,
        channels: u32,
    ) -> u32;
    
    fn xos_audio_player_queue_samples(
        player_id: u32,
        samples: *const f32,
        count: usize,
    ) -> std::os::raw::c_int;
    
    fn xos_audio_player_get_buffer_size(player_id: u32) -> u32;
    
    fn xos_audio_player_start(player_id: u32) -> std::os::raw::c_int;
    
    fn xos_audio_player_stop(player_id: u32) -> std::os::raw::c_int;
    
    fn xos_audio_player_destroy(player_id: u32);
}

