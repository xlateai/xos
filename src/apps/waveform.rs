use crate::audio;
use crate::engine::{Application, EngineState};

use std::time::Instant;

const WIDTH: u32 = 1024;
const HEIGHT: u32 = 256;
const LINE_THICKNESS: usize = 2;
const WAVEFORM_AMPLITUDE: f32 = 0.8;

const CHANNEL_COLORS: &[[u8; 3]] = &[
    [0, 255, 0], [255, 0, 0], [0, 150, 255], [255, 255, 0], [255, 0, 255],
    [0, 255, 255], [255, 165, 0], [180, 0, 255], [0, 128, 0], [128, 0, 0],
];
const DEFAULT_COLOR: [u8; 3] = [0, 255, 0];

pub struct Waveform {
    maybe_listener: Option<audio::AudioListener>,
    channel_waveforms: Vec<Vec<f32>>,
    last_tick: Instant,
}

impl Waveform {
    pub fn new() -> Self {
        Self {
            maybe_listener: None,
            channel_waveforms: Vec::new(),
            last_tick: Instant::now(),
        }
    }

    fn draw_background(&self, buffer: &mut [u8]) {
        for chunk in buffer.chunks_exact_mut(4) {
            chunk.copy_from_slice(&[0x10, 0x10, 0x18, 0xff]);
        }
    }

    fn draw_waveforms(&self, buffer: &mut [u8]) {
        let center_y = HEIGHT as usize / 2;
        let half_height = HEIGHT as f32 / 2.0 * WAVEFORM_AMPLITUDE;

        for (i, waveform) in self.channel_waveforms.iter().enumerate() {
            let [r, g, b] = *CHANNEL_COLORS.get(i).unwrap_or(&DEFAULT_COLOR);

            for x in 1..waveform.len() {
                let x1 = x - 1;
                let x2 = x;

                let y1 = (center_y as f32 - waveform[x1] * half_height) as isize;
                let y2 = (center_y as f32 - waveform[x2] * half_height) as isize;

                self.draw_line(buffer, x1 as isize, y1, x2 as isize, y2, r, g, b);
            }
        }
    }

    fn draw_line(&self, frame: &mut [u8], mut x0: isize, mut y0: isize, x1: isize, y1: isize, r: u8, g: u8, b: u8) {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            let offset = (LINE_THICKNESS as isize) / 2;
            for yo in -offset..=offset {
                for xo in -offset..=offset {
                    let x = x0 + xo;
                    let y = y0 + yo;
                    if x >= 0 && x < WIDTH as isize && y >= 0 && y < HEIGHT as isize {
                        let i = (y as usize * WIDTH as usize + x as usize) * 4;
                        frame[i] = r;
                        frame[i + 1] = g;
                        frame[i + 2] = b;
                        frame[i + 3] = 0xff;
                    }
                }
            }
            if x0 == x1 && y0 == y1 { break; }
            let e2 = 2 * err;
            if e2 >= dy {
                if x0 == x1 { break; }
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                if y0 == y1 { break; }
                err += dx;
                y0 += sy;
            }
        }
    }
}

impl Application for Waveform {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        let devices = audio::devices();
        let device = devices.get(1).ok_or("No audio device found")?;

        let buffer_duration = (WIDTH as f32) / 44100.0;
        let mut listener = audio::AudioListener::new(device, buffer_duration)
            .map_err(|e| format!("Error creating listener: {}", e))?;
        listener.record().map_err(|e| format!("Failed to start recording: {}", e))?;

        let num_channels = listener.buffer().channels() as usize;
        self.channel_waveforms = vec![vec![0.0; WIDTH as usize]; num_channels];
        self.maybe_listener = Some(listener);

        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let buffer = &mut state.frame.buffer;
        self.draw_background(buffer);

        let Some(listener) = &self.maybe_listener else { return };
        let all_samples = listener.get_samples_by_channel();

        for (i, samples) in all_samples.iter().enumerate() {
            if i < self.channel_waveforms.len() {
                let waveform = &mut self.channel_waveforms[i];
                *waveform = samples.clone();

                if waveform.len() > WIDTH as usize {
                    waveform.drain(0..(waveform.len() - WIDTH as usize));
                } else {
                    while waveform.len() < WIDTH as usize {
                        waveform.insert(0, 0.0);
                    }
                }
            }
        }

        self.draw_waveforms(buffer);
    }

    fn on_mouse_down(&mut self, _state: &mut EngineState) {}
    fn on_mouse_up(&mut self, _state: &mut EngineState) {}
    fn on_mouse_move(&mut self, _state: &mut EngineState) {}
}
