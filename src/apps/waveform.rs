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

        let device_index = 1;  // hard-coded device selector for now (needs UI kit)
        let device = devices.get(device_index).ok_or("No audio device found")?;

        let buffer_duration = 1.0; // 100ms buffer for better visualization
        let mut listener = audio::AudioListener::new(device, buffer_duration)?;
        listener.record()?;
        self.listener = Some(listener);
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let Some(listener) = &self.listener else { return };
        let buffer = &mut state.frame.buffer;
        let width = state.frame.width;
        let height = state.frame.height;

        // Clear screen to dark gray
        for pixel in buffer.chunks_exact_mut(4) {
            pixel[0] = 16;
            pixel[1] = 16;
            pixel[2] = 24;
            pixel[3] = 255;
        }

        let all_samples = listener.get_samples_by_channel();
        if all_samples.is_empty() {
            return;
        }

        let center_y = height as f32 / 2.0;
        let scale = center_y * 0.8; // amplitude scaling

        let samples = &all_samples[0];
        let step = samples.len().max(1) as f32 / width as f32;

        let mut prev_x = 0;
        let mut prev_y = center_y as isize;

        for x in 0..width as usize {
            let sample_index = (x as f32 * step) as usize;
            if sample_index >= samples.len() {
                break;
            }
            let y = (center_y - samples[sample_index] * scale) as isize;

            let dx = (x as isize - prev_x).abs();
            let dy = (y - prev_y).abs();
            let sx = if prev_x < x as isize { 1 } else { -1 };
            let sy = if prev_y < y { 1 } else { -1 };
            let mut err = dx - dy;
            let (mut cx, mut cy) = (prev_x, prev_y);

            while cx != x as isize || cy != y {
                if cx >= 0 && cx < width as isize && cy >= 0 && cy < height as isize {
                    let i = (cy as usize * width as usize + cx as usize) * 4;
                    buffer[i] = 0;
                    buffer[i + 1] = 255;
                    buffer[i + 2] = 0;
                    buffer[i + 3] = 255;
                }
                let e2 = 2 * err;
                if e2 > -dy { err -= dy; cx += sx; }
                if e2 < dx { err += dx; cy += sy; }
            }

            prev_x = x as isize;
            prev_y = y;
        }
    }

    fn on_mouse_down(&mut self, _state: &mut EngineState) {}
    fn on_mouse_up(&mut self, _state: &mut EngineState) {}
    fn on_mouse_move(&mut self, _state: &mut EngineState) {}
}
