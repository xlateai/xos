use cpal::traits::{DeviceTrait, HostTrait};
use std::fmt;

/// Represents an audio device with its details
pub struct AudioDevice {
    pub name: String,
    pub is_input: bool,
    pub is_output: bool,
    pub device_cpal: cpal::Device,
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

/// Get all available audio devices from the system
pub fn all() -> Vec<AudioDevice> {
    let host = cpal::default_host();
    let mut audio_devices = Vec::new();
    
    // Get input devices
    if let Ok(input_devices) = host.input_devices() {
        for device in input_devices {
            if let Ok(name) = device.name() {
                audio_devices.push(AudioDevice {
                    name,
                    is_input: true,
                    is_output: false,
                    device_cpal: device,
                });
            }
        }
    }
    
    // Get output devices
    if let Ok(output_devices) = host.output_devices() {
        for device in output_devices {
            if let Ok(name) = device.name() {
                // Check if this device is already in our list (as an input device)
                let existing_idx = audio_devices.iter().position(|d| d.name == name);
                
                if let Some(idx) = existing_idx {
                    // Update existing device to mark it as both input and output
                    audio_devices[idx].is_output = true;
                } else {
                    // Add as a new output-only device
                    audio_devices.push(AudioDevice {
                        name,
                        is_input: false,
                        is_output: true,
                        device_cpal: device,
                    });
                }
            }
        }
    }
    
    audio_devices
}

/// Print information about all available audio devices
pub fn print_all() {
    let devices = all();
    println!("XOS Audio: {} device(s) detected", devices.len());
    
    for (i, device) in devices.iter().enumerate() {
        println!("  {}: {}", i+1, device);
    }
}

/// Get the default input device if available
pub fn default_input() -> Option<AudioDevice> {
    let host = cpal::default_host();
    host.default_input_device().and_then(|device| {
        device.name().ok().map(|name| {
            AudioDevice {
                name,
                is_input: true,
                is_output: false,
                device_cpal: device,
            }
        })
    })
}

/// Get the default output device if available
pub fn default_output() -> Option<AudioDevice> {
    let host = cpal::default_host();
    host.default_output_device().and_then(|device| {
        device.name().ok().map(|name| {
            AudioDevice {
                name,
                is_input: false,
                is_output: true,
                device_cpal: device,
            }
        })
    })
}