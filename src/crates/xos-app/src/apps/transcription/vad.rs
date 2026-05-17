//! `xos app vad` — live mic / loopback with VAD level visualization.
//!
//! **iOS** `lib` builds omit ONNX Runtime / Silero; the level bar uses the same **mic RMS proxy** as
//! [`super::transcribe::TranscribeApp`]. For full **Silero** probability, use a desktop build
//! (default features include `silero_vad` + `ort`).

use crate::ai::transcription::{TranscriptionEngine, WhisperBackend};
use xos_core::engine::audio;
use xos_core::engine::{Application, EngineState};
use xos_core::rasterizer::fill;
use xos_core::rasterizer::text::{fonts, text_rasterization::TextRasterizer};
#[cfg(not(target_os = "ios"))]
use dialoguer::Select;
use std::io::{self, Write};

const BG: (u8, u8, u8, u8) = (12, 14, 20, 255);
const WAVE_BASELINE: (u8, u8, u8) = (120, 124, 132);
const WAVE_SILENT: (u8, u8, u8) = (230, 230, 235);
const WAVE_SPEECH: (u8, u8, u8) = (90, 230, 140);
const PANEL_BG: (u8, u8, u8) = (25, 28, 36);
const PANEL_FG: (u8, u8, u8) = (210, 214, 220);
const VAD_DISPLAY_THRESHOLD_DEFAULT: f32 = 0.12;
const VAD_PROB_EMA_ALPHA: f32 = 0.22;
const WAVE_SMOOTH_WINDOW: usize = 7;

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

fn vad_display_threshold() -> f32 {
    std::env::var("XOS_VAD_THRESHOLD")
        .ok()
        .and_then(|s| s.parse::<f32>().ok())
        .map(|v| v.clamp(0.0, 1.0))
        .unwrap_or(VAD_DISPLAY_THRESHOLD_DEFAULT)
}

struct VadStripCanvas {
    waveform_points: usize,
    /// Temporal EMA to smooth draws when the capture buffer updates in large chunks (esp. iOS).
    wave_display_ema: Vec<f32>,
}

const DEFAULT_WAVE_POINTS: usize = 640;
const AMPLIFICATION_FACTOR: f32 = 50.0;
const WAVE_BAND_FRAC: f32 = 0.62;
const WAVE_HEADROOM_FRAC: f32 = 0.08;
const LEVEL_PANEL_H_FRAC: f32 = 0.17;

impl VadStripCanvas {
    fn new() -> Self {
        Self {
            #[cfg(target_os = "ios")]
            waveform_points: 420,
            #[cfg(not(target_os = "ios"))]
            waveform_points: DEFAULT_WAVE_POINTS,
            wave_display_ema: Vec::new(),
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

    fn lerp_color(&self, t: f32, a: (u8, u8, u8), b: (u8, u8, u8)) -> (u8, u8, u8) {
        let t = t.clamp(0.0, 1.0);
        (
            (a.0 as f32 + (b.0 as f32 - a.0 as f32) * t) as u8,
            (a.1 as f32 + (b.1 as f32 - a.1 as f32) * t) as u8,
            (a.2 as f32 + (b.2 as f32 - a.2 as f32) * t) as u8,
        )
    }

    fn draw_pixel(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        x: i32,
        y: i32,
        c: (u8, u8, u8),
    ) {
        if x < 0 || x >= width as i32 || y < 0 || y >= height as i32 {
            return;
        }
        let i = ((y as u32 * width + x as u32) * 4) as usize;
        if i + 3 < buffer.len() {
            buffer[i] = c.0;
            buffer[i + 1] = c.1;
            buffer[i + 2] = c.2;
            buffer[i + 3] = 255;
        }
    }

    fn draw_line(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        c: (u8, u8, u8),
    ) {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let steps = dx.abs().max(dy.abs()).max(1.0) as i32;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let x = x0 + dx * t;
            let y = y0 + dy * t;
            self.draw_pixel(buffer, width, height, x.round() as i32, y.round() as i32, c);
        }
    }

    fn draw_line_thick(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        c: (u8, u8, u8),
    ) {
        self.draw_line(buffer, width, height, x0, y0, x1, y1, c);
        self.draw_line(buffer, width, height, x0, y0 - 1.0, x1, y1 - 1.0, c);
        self.draw_line(buffer, width, height, x0, y0 + 1.0, x1, y1 + 1.0, c);
    }

    fn draw_rect(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        c: (u8, u8, u8),
    ) {
        let sx = x0.floor().max(0.0) as i32;
        let sy = y0.floor().max(0.0) as i32;
        let ex = x1.ceil().min(width as f32) as i32;
        let ey = y1.ceil().min(height as f32) as i32;
        for y in sy..ey {
            for x in sx..ex {
                self.draw_pixel(buffer, width, height, x, y, c);
            }
        }
    }

    fn tick_draw(
        &mut self,
        state: &mut EngineState,
        listener: &audio::AudioListener,
        vad_prob: f32,
        vad_label: &TextRasterizer,
    ) {
        let shape = state.frame.shape();
        let width = shape[1] as u32;
        let height = shape[0] as u32;
        let buffer = state.frame_buffer_mut();

        let w = width as f32;
        let h = height as f32;
        let wave_top = h * WAVE_HEADROOM_FRAC;
        let wave_h = h * WAVE_BAND_FRAC;
        let wave_center_y = wave_top + wave_h * 0.5;
        let wave_half_amp = wave_h * 0.45;
        let panel_h = (h * LEVEL_PANEL_H_FRAC).max(36.0);
        let panel_top = h - panel_h - h * 0.03;
        let pad = (w * 0.03).max(12.0);
        let bar_x0 = pad;
        let bar_x1 = w - pad;
        let bar_y0 = panel_top + panel_h * 0.52;
        let bar_y1 = bar_y0 + panel_h * 0.24;

        let all_samples = listener.get_samples_by_channel();
        if all_samples.is_empty() {
            return;
        }
        let samples = &all_samples[0];
        if samples.is_empty() {
            return;
        }

        // Panel + level bar based on raw VAD probability [0, 1].
        self.draw_rect(
            buffer,
            width,
            height,
            bar_x0,
            panel_top,
            bar_x1,
            panel_top + panel_h,
            PANEL_BG,
        );
        self.draw_rect(
            buffer,
            width,
            height,
            bar_x0,
            bar_y0,
            bar_x1,
            bar_y1,
            WAVE_BASELINE,
        );
        let prob_color = self.lerp_color(vad_prob, WAVE_SILENT, WAVE_SPEECH);
        let fill_x1 = bar_x0 + (bar_x1 - bar_x0) * vad_prob.clamp(0.0, 1.0);
        self.draw_rect(
            buffer, width, height, bar_x0, bar_y0, fill_x1, bar_y1, prob_color,
        );

        // Baseline for center-origin waveform.
        self.draw_line(
            buffer,
            width,
            height,
            0.0,
            wave_center_y,
            width as f32 - 1.0,
            wave_center_y,
            WAVE_BASELINE,
        );

        // True left-to-right waveform from recent mono samples.
        let points = self
            .waveform_points
            .max(2)
            .min(width as usize)
            .min(samples.len());
        let start_idx = samples.len().saturating_sub(points);
        let active = &samples[start_idx..];
        let wave_color = self.lerp_color(vad_prob, WAVE_SILENT, WAVE_SPEECH);
        let x_scale = (width as f32 - 1.0) / (points.saturating_sub(1) as f32).max(1.0);

        let mut smooth = vec![0.0_f32; points];
        let half_w = WAVE_SMOOTH_WINDOW / 2;
        for i in 0..points {
            let from = i.saturating_sub(half_w);
            let to = (i + half_w + 1).min(points);
            let mut sum = 0.0_f32;
            let mut n = 0usize;
            for &s in &active[from..to] {
                sum += s;
                n += 1;
            }
            smooth[i] = if n > 0 { sum / n as f32 } else { 0.0 };
        }
        if self.wave_display_ema.len() != points {
            self.wave_display_ema = vec![0.0_f32; points];
            self.wave_display_ema.copy_from_slice(&smooth);
        } else {
            const WAVE_TEMPORAL_EMA: f32 = 0.42;
            let a = WAVE_TEMPORAL_EMA;
            for i in 0..points {
                self.wave_display_ema[i] = self.wave_display_ema[i] * (1.0 - a) + smooth[i] * a;
            }
        }

        let mut prev_x = 0.0;
        let mut prev_y = wave_center_y;
        for i in 0..points {
            let s = self.wave_display_ema[i];
            let amp = self.amplify_nonlinear(s).clamp(-1.0, 1.0);
            let x = i as f32 * x_scale;
            let y = wave_center_y - amp * wave_half_amp;
            if i > 0 {
                self.draw_line_thick(buffer, width, height, prev_x, prev_y, x, y, wave_color);
            }
            prev_x = x;
            prev_y = y;
        }

        // Blend pre-rasterized text.
        for character in &vad_label.characters {
            let char_x = bar_x0 + character.x;
            let char_y = panel_top + panel_h * 0.12 + character.y;
            let cw = character.width as usize;
            if cw == 0 {
                continue;
            }
            for (bitmap_y, row) in character.bitmap.chunks(cw).enumerate() {
                for (bitmap_x, &alpha) in row.iter().enumerate() {
                    if alpha == 0 {
                        continue;
                    }
                    let px = (char_x + bitmap_x as f32) as i32;
                    let py = (char_y + bitmap_y as f32) as i32;
                    if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                        let idx = ((py as u32 * width + px as u32) * 4) as usize;
                        let alpha_f = alpha as f32 / 255.0;
                        let inv = 1.0 - alpha_f;
                        buffer[idx] =
                            (PANEL_FG.0 as f32 * alpha_f + buffer[idx] as f32 * inv) as u8;
                        buffer[idx + 1] =
                            (PANEL_FG.1 as f32 * alpha_f + buffer[idx + 1] as f32 * inv) as u8;
                        buffer[idx + 2] =
                            (PANEL_FG.2 as f32 * alpha_f + buffer[idx + 2] as f32 * inv) as u8;
                        buffer[idx + 3] = 255;
                    }
                }
            }
        }
    }
}

pub struct VadApp {
    listener: Option<audio::AudioListener>,
    engine: TranscriptionEngine,
    strip: VadStripCanvas,
    vad_label: TextRasterizer,
    vad_prob_ema: f32,
}

impl VadApp {
    pub fn new() -> Self {
        let font = fonts::default_font();
        let mut vad_label = TextRasterizer::new(font, 24.0);
        vad_label.set_text("VAD: 0.000".to_string());
        Self {
            listener: None,
            engine: TranscriptionEngine::new_with_size_and_backend(
                None,
                transcribe_backend_from_env(),
            ),
            strip: VadStripCanvas::new(),
            vad_label,
            vad_prob_ema: 0.0,
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
            "vad: backend={backend_label} (XOS_TRANSCRIBE_BACKEND) · waveform + VAD level [0..1] · Esc to quit"
        );
        let _ = io::stdout().flush();
        println!(
            "vad: display threshold={:.2} (override with XOS_VAD_THRESHOLD=0..1)",
            vad_display_threshold()
        );
        let _ = io::stdout().flush();
        #[cfg(target_os = "ios")]
        eprintln!("vad: iOS: levels = mic RMS (Silero not in this build). Draggable VAD threshold: `xos app transcribe`.");

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
            let names: Vec<String> = input_devices.iter().map(|d| d.input_menu_label()).collect();
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

        // Silero ONNX is not in iOS `lib` builds; blend mic RMS or the bar and labels stay at ~0.
        let p_raw = {
            let p = self.engine.last_vad_speech_prob();
            #[cfg(target_os = "ios")]
            {
                p.max(super::transcribe::energy_speech_proxy(&channels))
            }
            #[cfg(not(target_os = "ios"))]
            {
                p
            }
        };
        self.vad_prob_ema =
            self.vad_prob_ema * (1.0 - VAD_PROB_EMA_ALPHA) + p_raw * VAD_PROB_EMA_ALPHA;
        let p = self.vad_prob_ema.clamp(0.0, 1.0);
        let l = self.listener.as_ref().expect("checked");
        let th = vad_display_threshold();
        let engine_th = vad_binary_threshold();
        let state_label = if p >= th { "speech" } else { "silence" };
        self.vad_label.set_text(format!(
            "VAD raw:{:.3}  ema:{:.3}  th:{:.2}  engine:{:.2}  {}",
            p_raw, p, th, engine_th, state_label
        ));
        let shape = state.frame.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        let font_size = (height.min(width) * 0.03).clamp(16.0, 36.0);
        self.vad_label.set_font_size(font_size);
        self.vad_label.tick(width, height);
        self.strip.tick_draw(state, l, p, &self.vad_label);
    }

    fn on_key_char(&mut self, _state: &mut EngineState, ch: char) {
        if ch == '\u{1b}' {
            self.pause_input();
            #[cfg(not(target_arch = "wasm32"))]
            xos_core::engine::native_engine::request_exit();
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
