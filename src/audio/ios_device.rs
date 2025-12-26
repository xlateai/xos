use std::fmt;

/// Represents an audio device with its details (iOS version)
/// Note: On iOS, we only support the built-in microphone for now
#[derive(Clone)]
pub struct AudioDevice {
    pub name: String,
    pub is_input: bool,
    pub is_output: bool,
    pub device_id: u32, // Simple ID for iOS devices
}

impl fmt::Display for AudioDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let device_type = match (self.is_input, self.is_output) {
            (true, true) => "Input/Output",
            (true, false) => "Input",
            (false, true) => "Output",
            (false, false) => "Unknown",
        };
        write!(f, "{} ({})", self.name, device_type)
    }
}

/// Get the default input device (iOS version)
#[cfg(target_os = "ios")]
pub fn default_input() -> Option<AudioDevice> {
    // On iOS, device_id 0 is typically the default input
    let devices = all();
    devices.into_iter().find(|d| d.is_input)
}

/// Get the default output device (iOS version)
#[cfg(target_os = "ios")]
pub fn default_output() -> Option<AudioDevice> {
    // On iOS, find the first output device (could be built-in or AirPods)
    let devices = all();
    devices.into_iter().find(|d| d.is_output)
}

/// Get all available audio devices from the system (iOS version)
#[cfg(target_os = "ios")]
pub fn all() -> Vec<AudioDevice> {
    // Call Swift to get device count
    let device_count = unsafe { xos_audio_get_device_count() };
    
    let mut audio_devices = Vec::new();
    
    for i in 0..device_count {
        // Get device name from Swift
        let name_ptr = unsafe { xos_audio_get_device_name(i) };
        if name_ptr.is_null() {
            continue;
        }
        
        let name = unsafe {
            let c_str = std::ffi::CStr::from_ptr(name_ptr);
            match c_str.to_str() {
                Ok(s) => s.to_string(),
                Err(_) => {
                    xos_audio_free_string(name_ptr);
                    continue;
                }
            }
        };
        
        // Free the C string
        unsafe { xos_audio_free_string(name_ptr); }
        
        // Get device capabilities
        let is_input = unsafe { xos_audio_device_is_input(i) != 0 };
        let is_output = unsafe { xos_audio_device_is_output(i) != 0 };
        
        audio_devices.push(AudioDevice {
            name,
            is_input,
            is_output,
            device_id: i,
        });
    }
    
    audio_devices
}

#[cfg(not(target_os = "ios"))]
pub fn all() -> Vec<AudioDevice> {
    Vec::new()
}

#[cfg(not(target_os = "ios"))]
pub fn default_input() -> Option<AudioDevice> {
    None
}

#[cfg(not(target_os = "ios"))]
pub fn default_output() -> Option<AudioDevice> {
    None
}

/// Print information about all available audio devices
pub fn print_all() {
    let devices = all();
    println!("XOS Audio: {} device(s) detected", devices.len());
    
    for (i, device) in devices.iter().enumerate() {
        println!("  {}: {}", i+1, device);
    }
}

// FFI declarations for iOS audio device functions
#[cfg(target_os = "ios")]
extern "C" {
    fn xos_audio_get_device_count() -> u32;
    fn xos_audio_get_device_name(device_id: u32) -> *const std::os::raw::c_char;
    fn xos_audio_device_is_input(device_id: u32) -> std::os::raw::c_int;
    fn xos_audio_device_is_output(device_id: u32) -> std::os::raw::c_int;
    fn xos_audio_free_string(ptr: *const std::os::raw::c_char);
}


