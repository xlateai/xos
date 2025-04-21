use super::wasm_device::AudioDevice;

#[derive(Clone)]
pub struct AudioListener;

impl AudioListener {
    pub fn new(_device: &AudioDevice, _buffer_duration_secs: f32) -> Result<Self, String> {
        Err("⚠️ AudioListener is not supported in WebAssembly (yet)".to_string())
    }

    pub fn record(&self) -> Result<(), String> {
        Err("⚠️ Cannot record in WASM".to_string())
    }

    pub fn get_samples_by_channel(&self) -> Vec<Vec<f32>> {
        vec![]
    }
}
