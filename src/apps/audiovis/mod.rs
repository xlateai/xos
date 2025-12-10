pub mod audiovis;
pub mod waveform;

#[cfg(not(target_arch = "wasm32"))]
mod audio_capture;

pub use audiovis::AudiovisApp;
pub use waveform::WaveformVisualizer;
