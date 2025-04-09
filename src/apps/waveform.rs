use crate::audio;
use crate::engine::{Application, EngineState};

pub struct Waveform {
    listener: Option<audio::AudioListener>,
}

impl Waveform {
    pub fn new() -> Self {
        Self {
            listener: None,
        }
    }
}

impl Application for Waveform {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        let devices = audio::devices();
        let device = devices.get(0).ok_or("No audio device found")?;

        let buffer_duration = 1.0; // ~50ms
        let mut listener = audio::AudioListener::new(device, buffer_duration)?;
        listener.record()?;
        self.listener = Some(listener);
        Ok(())
    }

    fn tick(&mut self, _state: &mut EngineState) {
        let Some(listener) = &self.listener else { return };
        let all_samples = listener.get_samples_by_channel();

        for (i, samples) in all_samples.iter().enumerate() {
            let count = samples.len();
            let peak = samples.iter().copied().fold(0.0, |a: f32, b: f32| a.max(b.abs()));
            let rms = (samples.iter().map(|x| x * x).sum::<f32>() / samples.len().max(1) as f32).sqrt();

            println!("Channel {}: {} samples | RMS: {:.3} | Peak: {:.3}", i, count, rms, peak);
        }
    }

    fn on_mouse_down(&mut self, _state: &mut EngineState) {}
    fn on_mouse_up(&mut self, _state: &mut EngineState) {}
    fn on_mouse_move(&mut self, _state: &mut EngineState) {}
}