//! XOS Audio Module
//! 
//! Provides cross-platform **audio input** and **audio output** (playback) functionality.
//! Source files keep historical names (`microphone`, `speakers`); import them as
//! [`input`] and [`output`] when you want IO-oriented naming.
//! 
//! ## Architecture
//! 
//! - [`microphone`] ([`input`]) — capture from CPAL **input** devices (mics, virtual cables),
//!   and on **Windows** from each **output** device as WASAPI **system audio** loopback.
//!   On **macOS**, use a virtual loopback driver (e.g. BlackHole) for system capture until
//!   a native loopback backend is wired in.
//! - [`speakers`] ([`output`]) — playback to output devices.
//! 
//! Each submodule handles platform-specific implementations (native/iOS/WASM) internally
//! using conditional compilation.
//! 
//! ## Common Types
//! 
//! ### AudioDevice
//! Represents a physical or virtual audio device. Contains:
//! - `name` - Human-readable device name
//! - `is_input` - Capture / input endpoint (mic, loopback, etc.)
//! - `is_output` - Playback endpoint
//! - Platform-specific device handle (CPAL device, iOS device_id, etc.)
//! 
//! Note: On some platforms (like macOS), a single physical device (e.g., AirPods) may appear
//! twice: once as an input device and once as an output device.

use std::fmt;

// Import platform-specific submodules
pub mod microphone;
pub mod speakers;
pub mod transcription;

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios"), target_os = "macos"))]
mod macos_sck;

// IO-oriented aliases (same modules; prefer these in new code)
pub use microphone as input;
pub use speakers as output;

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

    /// Windows: this row captures **system audio** from the paired output endpoint (WASAPI loopback).
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    pub wasapi_loopback: bool,

    /// macOS: ScreenCaptureKit system audio (not a CPAL device; `device_cpal` is a placeholder).
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    pub macos_sck_system_audio: bool,
    
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

/// Heuristic classification for **input** devices (see [`AudioDevice::input_kind_hint`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputDeviceKind {
    /// Typical mic or headset capture path.
    Microphone,
    /// Loopback, virtual cable, stereo mix, or similar (often used for system audio).
    LoopbackOrVirtual,
    Unknown,
}

impl InputDeviceKind {
    /// Short label for menus and compact UIs.
    pub fn as_menu_suffix(self) -> &'static str {
        match self {
            Self::Microphone => "microphone",
            Self::LoopbackOrVirtual => "loopback / system audio",
            Self::Unknown => "input",
        }
    }
}

impl AudioDevice {
    /// Best-effort hint from the device name. Virtual loopback drivers are still normal
    /// input devices at the OS level; this only helps UI and logging.
    pub fn input_kind_hint(&self) -> Option<InputDeviceKind> {
        if !self.is_input {
            return None;
        }
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
        if self.wasapi_loopback || self.macos_sck_system_audio {
            return Some(InputDeviceKind::LoopbackOrVirtual);
        }
        let n = self.name.to_lowercase();
        if n.contains("blackhole")
            || n.contains("loopback")
            || n.contains("stereo mix")
            || n.contains("vb-audio")
            || n.contains("virtual cable")
            || n.contains("cable output")
            || n.contains("wave out mix")
            || n.contains("what u hear")
        {
            Some(InputDeviceKind::LoopbackOrVirtual)
        } else if n.contains("microphone")
            || n.contains("mic ")
            || n.contains(" mic")
            || n.contains("headset")
            || n.contains("headphone")
        {
            Some(InputDeviceKind::Microphone)
        } else {
            Some(InputDeviceKind::Unknown)
        }
    }

    /// Line shown in device pickers. Appends a hint only when it disambiguates (e.g. loopback vs mic).
    /// Typical microphones use the OS name alone so you do not get `Foo Microphone (microphone)`.
    pub fn input_menu_label(&self) -> String {
        if !self.is_input {
            return self.name.clone();
        }
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
        if self.wasapi_loopback || self.macos_sck_system_audio {
            return self.name.clone();
        }
        let kind = self.input_kind_hint().unwrap_or(InputDeviceKind::Unknown);
        match kind {
            InputDeviceKind::Microphone => self.name.clone(),
            InputDeviceKind::LoopbackOrVirtual => {
                format!("{} ({})", self.name, InputDeviceKind::LoopbackOrVirtual.as_menu_suffix())
            }
            InputDeviceKind::Unknown => {
                format!("{} ({})", self.name, InputDeviceKind::Unknown.as_menu_suffix())
            }
        }
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
            wasapi_loopback: device.wasapi_loopback,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
            macos_sck_system_audio: device.macos_sck_system_audio,
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
            wasapi_loopback: false,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
            macos_sck_system_audio: false,
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
            println!("  {}: {}", i+1, device.input_menu_label());
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
