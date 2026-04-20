//! `xos app vad` — live mic / loopback with Silero speech probability: status circle + thin waveform strip.

use crate::ai::transcription::{TranscriptionEngine, WhisperBackend};
use crate::engine::audio;
use crate::engine::{Application, EngineState};
use crate::rasterizer::{circles, fill};
#[cfg(not(target_os = "ios"))]
use dialoguer::Select;
use std::io::{self, Write};

const BG: (u8, u8, u8, u8) = (12, 14, 20, 255);
const CIRCLE_SILENT: [u8; 4] = [96, 96, 100, 255];
const CIRCLE_SPEECH: [u8; 4] = [72, 210, 120, 255];
const WAVE_SILENT: (u8, u8, u8) = (220, 220, 225);
const WAVE_SPEECH: (u8, u8, u8) = (110, 230, 150);

fn transcribe_backend_from_env() -> WhisperBackend {
    std::env::var("XOS_TRANSCRIBE_BACKEND")
        .ok()
        .and_then(|s| WhisperBackend::from_str(&s))
        .unwrap_or(WhisperBackend::Ct2)
}

fn vad_binary_threshold() -> f32 {
    #[cfg(all(
        feature = "silero_vad",
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    {
        return crate::ai::transcription::SILERO_VAD_SPEECH_THRESHOLD;
    }
    #[cfg(not(all(
        feature = "silero_vad",
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    )))]
    {
        0.35
    }
}

/// Bottom strip: scrolling horizontal lines, amplitude from channel 0, color by speech vs silence.
struct VadStripCanvas {
    sample_buffer: Vec<f32>,
    speech_buffer: Vec<bool>,
    buffer_index: usize,
    lines_to_add: f32,
}

const STRIP_LINES: usize = 256;
const BASELINE_LENGTH: f32 = 0.02;
const MAX_EXTRA_LENGTH: f32 = 0.42;
const LINE_THICKNESS: f32 = 0.0022;
const PROPAGATION_TIME_SECS: f32 = 1.0;
const AMPLIFICATION_FACTOR: f32 = 50.0;
const SAMPLE_RATE: f32 = 44100.0;
const TARGET_FPS: f32 = 60.0;
const BAND_FRAC: f32 = 0.09;

impl VadStripCanvas {
    fn new() -> Self {
        Self {
            sample_buffer: vec![0.0; STRIP_LINES],
            speech_buffer: vec![false; STRIP_LINES],
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

    fn tick_draw(
        &mut self,
        state: &mut EngineState,
        listener: &audio::AudioListener,
        speech_now: bool,
    ) {
        let shape = state.frame.shape();
        let width = shape[1] as u32;
        let height = shape[0] as u32;
        let buffer = state.frame_buffer_mut();

        let band_top = (height as f32 * (1.0 - BAND_FRAC)).max(0.0);
        let band_h = height as f32 - band_top;

        let all_samples = listener.get_samples_by_channel();
        if all_samples.is_empty() {
            return;
        }
        let samples = &all_samples[0];
        if samples.is_empty() {
            return;
        }

        let lines_per_frame = STRIP_LINES as f32 / (PROPAGATION_TIME_SECS * TARGET_FPS);
        self.lines_to_add += lines_per_frame;
        let lines_to_process = (self.lines_to_add.floor() as usize).min(20);
        self.lines_to_add -= lines_to_process as f32;
        if lines_to_process == 0 {
            return;
        }

        let samples_per_line = ((SAMPLE_RATE * PROPAGATION_TIME_SECS) / STRIP_LINES as f32) as usize;
        let total_samples = samples.len();

        for _ in 0..lines_to_process {
            let window_size = samples_per_line.min(total_samples);
            let start_idx = total_samples.saturating_sub(window_size);
            if start_idx >= total_samples {
                break;
            }
            let chunk_samples = &samples[start_idx..total_samples];
            let mut rms_sum = 0.0f32;
            for &sample in chunk_samples {
                rms_sum += sample * sample;
            }
            let rms = (rms_sum / chunk_samples.len() as f32).sqrt();
            let amplified = self.amplify_nonlinear(rms);
            let normalized = amplified.clamp(0.0, 1.0);
            self.sample_buffer[self.buffer_index] = normalized;
            self.speech_buffer[self.buffer_index] = speech_now;
            self.buffer_index = (self.buffer_index + 1) % STRIP_LINES;
        }

        let spacing = band_h / STRIP_LINES as f32;
        let thickness_px = LINE_THICKNESS * band_h.max(1.0);
        for line_idx in 0..STRIP_LINES {
            let buf_idx = (self.buffer_index + line_idx) % STRIP_LINES;
            let amp = self.sample_buffer[buf_idx];
            let half_len = (BASELINE_LENGTH + amp * MAX_EXTRA_LENGTH) * width as f32 * 0.5;
            let y_from_band_bottom = line_idx as f32 * spacing;
            let y = band_top + band_h - y_from_band_bottom - spacing * 0.5;
            let y = y.clamp(0.0, height as f32 - 1.0);
            let color = if self.speech_buffer[buf_idx] {
                WAVE_SPEECH
            } else {
                WAVE_SILENT
            };
            self.draw_horizontal_line(buffer, width, height, y, half_len, color, thickness_px);
        }
    }
}

pub struct VadApp {
    listener: Option<audio::AudioListener>,
    engine: TranscriptionEngine,
    strip: VadStripCanvas,
}

impl VadApp {
    pub fn new() -> Self {
        Self {
            listener: None,
            engine: TranscriptionEngine::new_with_size_and_backend(None, transcribe_backend_from_env()),
            strip: VadStripCanvas::new(),
        }
    }

    fn pause_input(&self) {
        if let Some(l) = &self.listener {
            let _ = l.pause();
        }
    }
}

impl Drop for VadApp {
    fn drop(&mut self) {
        self.pause_input();
    }
}

impl Application for VadApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        let backend = transcribe_backend_from_env();
        let backend_label = match backend {
            WhisperBackend::Ct2 => "ct2",
            WhisperBackend::Burn => "burn",
        };
        println!(
            "vad: backend={backend_label} (XOS_TRANSCRIBE_BACKEND) · Silero circle + strip · Esc to quit"
        );
        let _ = io::stdout().flush();

        let all_devices = audio::devices();
        let input_devices: Vec<_> = all_devices.into_iter().filter(|d| d.is_input).collect();

        if input_devices.is_empty() {
            return Err("No audio input devices found. On Windows, choose “… (system audio)” for built-in capture. Otherwise use a mic or a loopback driver (e.g. BlackHole on macOS).".to_string());
        }

        #[cfg(target_os = "ios")]
        {
            let device = input_devices.first().ok_or("No input devices available")?;
            #[cfg(all(
                feature = "whisper",
                not(target_os = "ios"),
                not(target_arch = "wasm32")
            ))]
            let buffer_duration = 10.0_f32;
            #[cfg(not(all(
                feature = "whisper",
                not(target_os = "ios"),
                not(target_arch = "wasm32")
            )))]
            let buffer_duration = 3.0_f32;
            let listener = audio::AudioListener::new(device, buffer_duration)?;
            listener.record()?;
            println!(
                "vad: input {} @ {} Hz",
                device.name,
                listener.buffer().sample_rate()
            );
            let _ = io::stdout().flush();
            self.engine
                .set_device_hint(device.name.as_str(), listener.buffer().sample_rate());
            self.listener = Some(listener);
            Ok(())
        }

        #[cfg(not(target_os = "ios"))]
        {
            let names: Vec<String> = input_devices
                .iter()
                .map(|d| d.input_menu_label())
                .collect();
            let selection = Select::new()
                .with_prompt("Select audio input (mic or loopback for system mix)")
                .items(&names)
                .default(0)
                .interact()
                .map_err(|e| format!("Device selection failed: {e}"))?;

            let device = input_devices
                .get(selection)
                .ok_or_else(|| "Selected device not found".to_string())?;

            #[cfg(all(
                feature = "whisper",
                not(target_os = "ios"),
                not(target_arch = "wasm32")
            ))]
            let buffer_duration = 10.0_f32;
            #[cfg(not(all(
                feature = "whisper",
                not(target_os = "ios"),
                not(target_arch = "wasm32")
            )))]
            let buffer_duration = 3.0_f32;
            let listener = audio::AudioListener::new(device, buffer_duration)?;
            listener.record()?;
            println!(
                "vad: input {} @ {} Hz",
                device.name,
                listener.buffer().sample_rate()
            );
            let _ = io::stdout().flush();
            self.engine
                .set_device_hint(device.name.as_str(), listener.buffer().sample_rate());
            self.listener = Some(listener);
            Ok(())
        }
    }

    fn tick(&mut self, state: &mut EngineState) {
        fill(&mut state.frame, BG);

        if self.listener.is_none() {
            return;
        }

        let (channels, sr, ingested) = {
            let l = self.listener.as_ref().expect("checked");
            let buf = l.buffer();
            (
                l.get_samples_by_channel(),
                buf.sample_rate(),
                buf.ingested_frame_count(),
            )
        };

        self.engine.process_snapshot(sr, &channels, ingested);

        let p = self.engine.last_vad_speech_prob();
        let th = vad_binary_threshold();
        let speech = p >= th;

        let l = self.listener.as_ref().expect("checked");
        self.strip.tick_draw(state, l, speech);

        let shape = state.frame.shape();
        let w = shape[1] as f32;
        let h = shape[0] as f32;
        let cx = w * 0.5;
        let cy = h * (0.5 - BAND_FRAC * 0.35);
        let r = (w.min(h)) * 0.14;
        let circle_color = if speech { CIRCLE_SPEECH } else { CIRCLE_SILENT };
        let _ = circles(
            &mut state.frame,
            &[(cx, cy)],
            &[r],
            std::slice::from_ref(&circle_color),
        );
    }

    fn on_key_char(&mut self, _state: &mut EngineState, ch: char) {
        if ch == '\u{1b}' {
            self.pause_input();
            #[cfg(not(target_arch = "wasm32"))]
            crate::engine::native_engine::request_exit();
        }
    }

    fn prepare_shutdown(&mut self, _state: &mut EngineState) {
        if let Some(listener) = self.listener.take() {
            let _ = listener.pause();
        }
    }

    fn on_mouse_down(&mut self, _state: &mut EngineState) {}

    fn on_mouse_up(&mut self, _state: &mut EngineState) {}

    fn on_mouse_move(&mut self, _state: &mut EngineState) {}
}
