//! XOS Audio Module
//! 
//! Provides cross-platform audio input (microphone) and output (speakers) functionality.
//! 
//! ## Architecture
//! 
//! This module is organized into two main submodules:
//! - `microphone` - Audio input/capture functionality
//! - `speakers` - Audio output/playback functionality
//! 
//! Each submodule handles platform-specific implementations (native/iOS/WASM) internally
//! using conditional compilation.
//! 
//! ## Common Types
//! 
//! ### AudioDevice
//! Represents a physical or virtual audio device. Contains:
//! - `name` - Human-readable device name
//! - `is_input` - Whether device supports input (microphone)
//! - `is_output` - Whether device supports output (speakers)
//! - Platform-specific device handle (CPAL device, iOS device_id, etc.)
//! 
//! Note: On some platforms (like macOS), a single physical device (e.g., AirPods) may appear
//! twice: once as an input device and once as an output device.

use std::fmt;

// Import platform-specific submodules
pub mod microphone;
pub mod speakers;
pub mod transcription;

// Re-export key types for convenience
pub use microphone::{AudioListener, default_input, all_input_devices, print_input_devices};
pub use speakers::{AudioPlayer, default_output, all_output_devices, print_output_devices};

// ================================================================================================
// COMMON AUDIO DEVICE TYPE
// ================================================================================================

/// Represents an audio device (unified for both input and output)
/// 
/// This is the main type used to represent audio devices across the codebase.
/// Platform-specific implementations in microphone.rs and speakers.rs have their own
/// AudioDevice types, but they're compatible and can be converted if needed.
#[derive(Clone)]
pub struct AudioDevice {
    pub name: String,
    pub is_input: bool,
    pub is_output: bool,
    
    // Platform-specific device handles
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    pub device_cpal: cpal::Device,
    
    #[cfg(target_os = "ios")]
    pub device_id: u32,
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

// ================================================================================================
// CONVENIENCE FUNCTIONS
// ================================================================================================

/// Get all available audio devices (both input and output)
/// 
/// Note: On some platforms, a single physical device (like AirPods) will appear twice:
/// once as an input device and once as an output device. This is intentional as they
/// require separate device handles for input vs output operations.
pub fn devices() -> Vec<AudioDevice> {
    let mut all_devices = Vec::new();
    
    // Add all input devices
    for device in all_input_devices() {
        all_devices.push(AudioDevice {
            name: device.name.clone(),
            is_input: device.is_input,
            is_output: device.is_output,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
            device_cpal: device.device_cpal.clone(),
            #[cfg(target_os = "ios")]
            device_id: device.device_id,
        });
    }
    
    // Add all output devices
    for device in all_output_devices() {
        all_devices.push(AudioDevice {
            name: device.name.clone(),
            is_input: device.is_input,
            is_output: device.is_output,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
            device_cpal: device.device_cpal.clone(),
            #[cfg(target_os = "ios")]
            device_id: device.device_id,
        });
    }
    
    all_devices
}

/// Print information about all available audio devices
pub fn print_devices() {
    let all_devices = devices();
    println!("XOS Audio: {} total device(s) detected", all_devices.len());
    println!();
    
    // Print input devices
    let input_devices: Vec<_> = all_devices.iter().filter(|d| d.is_input).collect();
    if !input_devices.is_empty() {
        println!("Input Devices ({}):", input_devices.len());
        for (i, device) in input_devices.iter().enumerate() {
            println!("  {}: {}", i+1, device.name);
        }
        println!();
    }
    
    // Print output devices
    let output_devices: Vec<_> = all_devices.iter().filter(|d| d.is_output).collect();
    if !output_devices.is_empty() {
        println!("Output Devices ({}):", output_devices.len());
        for (i, device) in output_devices.iter().enumerate() {
            println!("  {}: {}", i+1, device.name);
        }
    }
}
