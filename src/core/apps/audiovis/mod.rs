pub mod audiovis;
pub mod waveform;
pub mod convolutional_waveform;
pub mod media_control_bar;

#[cfg(not(target_arch = "wasm32"))]
pub mod audio_capture;

pub use audiovis::AudiovisApp;
pub use waveform::WaveformVisualizer;
pub use convolutional_waveform::ConvolutionalWaveform;
