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

fn draw_line(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    x0: isize,
    y0: isize,
    x1: isize,
    y1: isize,
    thickness: usize,
) {
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx - dy;
    let mut x = x0;
    let mut y = y0;

    while x != x1 || y != y1 {
        for tx in 0..thickness {
            for ty in 0..thickness {
                let px = x + tx as isize;
                let py = y + ty as isize;
                if px >= 0 && py >= 0 && (px as u32) < width && (py as u32) < height {
                    let i = (py as usize * width as usize + px as usize) * 4;
                    buffer[i] = 0;
                    buffer[i + 1] = 255;
                    buffer[i + 2] = 0;
                    buffer[i + 3] = 255;
                }
            }
        }

        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }
    }
}

impl Application for Waveform {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        let devices = audio::devices();
        if devices.is_empty() {
            return Err("⚠️ No audio input devices found.".to_string());
        }

        crate::print("🔊 Available devices:");
        for (i, d) in devices.iter().enumerate() {
            crate::print(&format!("  [{}] {}", i, d.name));
        }

        let device_index = 0;
        let device = devices.get(device_index).ok_or("No audio device found")?;

        let buffer_duration = 1.0;
        let listener = audio::AudioListener::new(device, buffer_duration)?;
        listener.record()?;
        self.listener = Some(listener);
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let Some(listener) = &self.listener else { return };
        let shape = state.frame.shape();
        let width = shape[1] as u32;
        let height = shape[0] as u32;
        let buffer = state.frame_buffer_mut();

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

        let samples = &all_samples[0];
        let vertical = height > width;

        let (len, scale, center) = if vertical {
            (height, width as f32 * 0.5 * 0.8, width as f32 * 0.5)
        } else {
            (width, height as f32 * 0.5 * 0.8, height as f32 * 0.5)
        };

        let step = samples.len().max(1) as f32 / len as f32;
        let stride = 2;
        let thickness = 2;

        let mut prev = None;

        for i in (0..len).step_by(stride as usize) {
            let sample_index = (i as f32 * step) as usize;
            if sample_index >= samples.len() {
                break;
            }

            let offset = samples[sample_index] * scale;
            let (x, y) = if vertical {
                ((center + offset) as isize, i as isize)
            } else {
                (i as isize, (center - offset) as isize)
            };

            if let Some((prev_x, prev_y)) = prev {
                draw_line(buffer, width, height, prev_x, prev_y, x, y, thickness);
            }

            prev = Some((x, y));
        }
    }

    fn on_mouse_down(&mut self, _state: &mut EngineState) {}
    fn on_mouse_up(&mut self, _state: &mut EngineState) {}
    fn on_mouse_move(&mut self, _state: &mut EngineState) {}
}
