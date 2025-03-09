mod buffer;
mod device;
mod listener;

pub use listener::AudioListener;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Once};

static INIT: Once = Once::new();
static mut LISTENER: Option<Arc<Mutex<AudioListener>>> = None;

/// Returns the global AudioListener instance containing all system audio devices.
/// The AudioListener is created only once and shared between all calls.
pub fn get_listener() -> Result<Arc<Mutex<AudioListener>>, String> {
    unsafe {
        INIT.call_once(|| {
            match AudioListener::new() {
                Ok(mut new_listener) => {
                    // Start listening on devices right away
                    if let Err(e) = new_listener.start_listening() {
                        eprintln!("Warning: Failed to start some audio devices: {}", e);
                    }
                    LISTENER = Some(Arc::new(Mutex::new(new_listener)));
                }
                Err(e) => {
                    eprintln!("Error creating AudioListener: {}", e);
                    // Initialize with empty listener to avoid panics
                    LISTENER = None;
                }
            }
        });

        match &LISTENER {
            Some(listener) => Ok(Arc::clone(listener)),
            None => Err("Failed to initialize audio listener".to_string()),
        }
    }
}

/// Convenience function to get all samples from all devices with their colors
pub fn get_all_samples() -> Result<HashMap<String, (Vec<Vec<f32>>, (u8, u8, u8))>, String> {
    let listener = get_listener()?;
    let listener = listener.lock().unwrap();
    Ok(listener.get_samples())
}

/// Convenience function to get all device names
pub fn get_device_names() -> Result<Vec<String>, String> {
    let listener = get_listener()?;
    let listener = listener.lock().unwrap();
    Ok(listener.get_device_names())
}