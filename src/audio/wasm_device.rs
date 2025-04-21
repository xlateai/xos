// audio/wasm_device.rs

use std::fmt;

/// Stub for unsupported WASM audio input
pub struct AudioDevice {
    pub name: String,
    pub is_input: bool,
    pub is_output: bool,
}

impl fmt::Display for AudioDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (Unavailable on Web)", self.name)
    }
}

pub fn all() -> Vec<AudioDevice> {
    vec![] // no native devices on wasm
}

pub fn print_all() {
    println!("⚠️ Audio devices are not supported in WebAssembly yet.");
}
