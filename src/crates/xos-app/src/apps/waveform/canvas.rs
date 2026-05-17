//! Full-frame scrolling RMS waveform (shared by the `waveform` app and `transcribe`).

use xos_core::engine::audio::AudioListener;
use xos_core::engine::EngineState;

const NUM_LINES: usize = 512;
const BASELINE_LENGTH: f32 = 0.012;
const MAX_EXTRA_LENGTH: f32 = 0.678;
const LINE_THICKNESS: f32 = 0.003;
const PROPAGATION_TIME_SECS: f32 = 1.0;
const AMPLIFICATION_FACTOR: f32 = 50.0;
const SAMPLE_RATE: f32 = 44100.0;
const TARGET_FPS: f32 = 60.0;

/// Draws the classic xOS scrolling horizontal-line waveform into [`EngineState::frame`].
pub struct WaveformCanvas {
    sample_buffer: Vec<f32>,
    color_buffer: Vec<(u8, u8, u8)>,
    buffer_index: usize,
    lines_to_add: f32,
}

impl WaveformCanvas {
    pub fn new() -> Self {
        Self {
            sample_buffer: vec![0.0; NUM_LINES],
            color_buffer: vec![(128, 128, 128); NUM_LINES],
            buffer_index: 0,
            lines_to_add: 0.0,
        }
    }

    fn amplify_nonlinear(&self, value: f32) -> f32 {
        let abs_val = value.abs();
        let boosted = abs_val * AMPLIFICATION_FACTOR;
        let amplified = if boosted < 0.1 {
            boosted * 2.0
        } else if boosted < 1.0 {
            0.2 + (boosted - 0.1) * 1.5
        } else {
            0.2 + 1.35 + (boosted - 1.0).ln().max(0.0) * 0.4
        };
        if value < 0.0 {
            -amplified
        } else {
            amplified
        }
    }

    fn draw_idle_baseline(&self, buffer: &mut [u8], width: u32, height: u32) {
        let mid = height as f32 * 0.5;
        let color = (90, 110, 140);
        self.draw_horizontal_line(buffer, width, height, mid, BASELINE_LENGTH, color, LINE_THICKNESS);
        self.draw_horizontal_line(
            buffer,
            width,
            height,
            mid,
            BASELINE_LENGTH * 0.35,
            (60, 72, 96),
            LINE_THICKNESS * 0.75,
        );
    }

    fn draw_horizontal_line(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        y: f32,
        half_length: f32,
        color: (u8, u8, u8),
        thickness: f32,
    ) {
        let center_x = width as f32 * 0.5;
        let x0 = (center_x - half_length).max(0.0);
        let x1 = (center_x + half_length).min(width as f32 - 1.0);
        let y_start = (y - thickness * 0.5).max(0.0) as u32;
        let y_end = (y + thickness * 0.5).min(height as f32 - 1.0) as u32;
        for y_pos in y_start..=y_end {
            for x_pos in x0 as u32..=x1 as u32 {
                let i = (y_pos * width + x_pos) as usize * 4;
                if i + 3 < buffer.len() {
                    buffer[i] = color.0;
                    buffer[i + 1] = color.1;
                    buffer[i + 2] = color.2;
                    buffer[i + 3] = 255;
                }
            }
        }
    }

    /// Clears the frame and draws the waveform from `listener` (channel 0).
    pub fn tick_draw(&mut self, state: &mut EngineState, listener: &AudioListener) {
        let shape = state.frame.shape();
        let width = shape[1] as u32;
        let height = shape[0] as u32;
        let buffer = state.frame_buffer_mut();

        for pixel in buffer.chunks_exact_mut(4) {
            pixel[0] = 8;
            pixel[1] = 10;
            pixel[2] = 15;
            pixel[3] = 255;
        }

        let all_samples = listener.get_samples_by_channel();
        if all_samples.is_empty() {
            self.draw_idle_baseline(buffer, width, height);
            return;
        }
        let samples = &all_samples[0];
        if samples.is_empty() {
            self.draw_idle_baseline(buffer, width, height);
            return;
        }

        let lines_per_frame = NUM_LINES as f32 / (PROPAGATION_TIME_SECS * TARGET_FPS);
        self.lines_to_add += lines_per_frame;
        let lines_to_process = (self.lines_to_add.floor() as usize).min(20);
        self.lines_to_add -= lines_to_process as f32;
        if lines_to_process == 0 {
            return;
        }

        let samples_per_line = ((SAMPLE_RATE * PROPAGATION_TIME_SECS) / NUM_LINES as f32) as usize;
        let total_samples = samples.len();

        for _ in 0..lines_to_process {
            let window_size = samples_per_line.min(total_samples);
            let start_idx = total_samples.saturating_sub(window_size);
            if start_idx >= total_samples {
                break;
            }
            let mut rms_sum = 0.0f32;
            let chunk_samples = &samples[start_idx..total_samples];
            for &sample in chunk_samples {
                rms_sum += sample * sample;
            }
            let rms = (rms_sum / chunk_samples.len() as f32).sqrt();
            let amplified = self.amplify_nonlinear(rms);
            let normalized = amplified.clamp(0.0, 1.0);
            self.sample_buffer[self.buffer_index] = normalized;
            let amp = normalized;
            let color = if amp < 0.15 {
                let brightness = (180.0 + amp / 0.15 * 75.0) as u8;
                (brightness, brightness, brightness)
            } else if amp < 0.4 {
                let t = (amp - 0.15) / 0.25;
                let r = (255.0 - t * 155.0) as u8;
                (r, 255, 255)
            } else if amp < 0.65 {
                let t = (amp - 0.4) / 0.25;
                let r = (100.0 - t * 100.0) as u8;
                let b = (255.0 - t * 155.0) as u8;
                (r, 255, b)
            } else if amp < 0.85 {
                let t = (amp - 0.65) / 0.2;
                let r = (t * 255.0) as u8;
                (r, 255, 0)
            } else {
                let t = (amp - 0.85) / 0.15;
                let g = (255.0 - t * 100.0) as u8;
                (255, g, 0)
            };
            self.color_buffer[self.buffer_index] = color;
            self.buffer_index = (self.buffer_index + 1) % NUM_LINES;
        }

        let spacing = height as f32 / NUM_LINES as f32;
        let thickness_px = LINE_THICKNESS * height as f32;
        for line_idx in 0..NUM_LINES {
            let buf_idx = (self.buffer_index + line_idx) % NUM_LINES;
            let amp = self.sample_buffer[buf_idx];
            let half_len = (BASELINE_LENGTH + amp * MAX_EXTRA_LENGTH) * width as f32 * 0.5;
            let y = height as f32 - (line_idx as f32 * spacing);
            let color = self.color_buffer[buf_idx];
            self.draw_horizontal_line(buffer, width, height, y, half_len, color, thickness_px);
        }
    }
}
