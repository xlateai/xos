use crate::ai::transcription::{TranscriptionEngine, WhisperBackend};
use crate::engine::audio;
use crate::engine::{Application, EngineState};
use crate::rasterizer::fill;
use crate::rasterizer::text::{fonts, text_rasterization::TextRasterizer};
#[cfg(not(target_os = "ios"))]
use dialoguer::Select;
use fontdue::Font;
use std::collections::VecDeque;
use std::io::{self, Write};

const BG: (u8, u8, u8, u8) = (12, 14, 20, 255);
const WAVE_BASELINE: (u8, u8, u8) = (120, 124, 132);
const WAVE_SILENT: (u8, u8, u8) = (230, 230, 235);
const WAVE_SPEECH: (u8, u8, u8) = (90, 230, 140);
const PANEL_BG: (u8, u8, u8) = (25, 28, 36);
const PANEL_FG: (u8, u8, u8) = (210, 214, 220);
const STATE_ACTIVE: (u8, u8, u8) = (92, 230, 142);
const STATE_SILENCE: (u8, u8, u8) = (168, 172, 182);
const TEXTBOX_BG: (u8, u8, u8) = (18, 21, 28);
const TEXTBOX_BORDER: (u8, u8, u8) = (52, 58, 70);
const TEXTBOX_SCROLL: (u8, u8, u8) = (90, 98, 114);
const TEXT_COLOR: (u8, u8, u8) = (230, 236, 240);
const THRESHOLD_DEFAULT: f32 = 0.30;
const THRESHOLD_MIN: f32 = 0.01;
const THRESHOLD_MAX: f32 = 1.0;
const VAD_PROB_EMA_ALPHA: f32 = 0.22;
const WAVE_SMOOTH_WINDOW: usize = 7;
const SPEECH_START_FRAMES: u32 = 3;
const SILENCE_COMMIT_FRAMES: u32 = 8;
const DEFAULT_WAVE_POINTS: usize = 640;
const AMPLIFICATION_FACTOR: f32 = 50.0;
const WAVE_BAND_FRAC: f32 = 0.375;
const WAVE_HEADROOM_FRAC: f32 = 0.0;
const LEVEL_PANEL_H_FRAC: f32 = 0.17;
const TEXTBOX_BOTTOM_GAP_FRAC: f32 = 0.015;

fn transcribe_backend_from_env() -> WhisperBackend {
    std::env::var("XOS_TRANSCRIBE_BACKEND")
        .ok()
        .and_then(|s| WhisperBackend::from_str(&s))
        .unwrap_or(WhisperBackend::Ct2)
}

fn clamp_threshold(v: f32) -> f32 {
    v.clamp(THRESHOLD_MIN, THRESHOLD_MAX)
}

#[derive(Clone, Copy, Debug, Default)]
struct UiBounds {
    transcript: Option<(f32, f32, f32, f32)>,
    slider: Option<(f32, f32, f32, f32)>,
}

struct VisualCanvas {
    waveform_points: usize,
}

impl VisualCanvas {
    fn new() -> Self {
        Self {
            waveform_points: DEFAULT_WAVE_POINTS,
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
        if value < 0.0 { -amplified } else { amplified }
    }

    fn lerp_color(&self, t: f32, a: (u8, u8, u8), b: (u8, u8, u8)) -> (u8, u8, u8) {
        let t = t.clamp(0.0, 1.0);
        (
            (a.0 as f32 + (b.0 as f32 - a.0 as f32) * t) as u8,
            (a.1 as f32 + (b.1 as f32 - a.1 as f32) * t) as u8,
            (a.2 as f32 + (b.2 as f32 - a.2 as f32) * t) as u8,
        )
    }

    fn draw_pixel(&self, buffer: &mut [u8], width: u32, height: u32, x: i32, y: i32, c: (u8, u8, u8)) {
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

    fn blend_text(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        rasterizer: &TextRasterizer,
        ox: f32,
        oy: f32,
        rgb: (u8, u8, u8),
        alpha_mul: f32,
    ) {
        for character in &rasterizer.characters {
            let char_x = ox + character.x;
            let char_y = oy + character.y;
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
                        let a = (alpha as f32 / 255.0) * alpha_mul.clamp(0.0, 1.0);
                        let inv = 1.0 - a;
                        buffer[idx] = (rgb.0 as f32 * a + buffer[idx] as f32 * inv) as u8;
                        buffer[idx + 1] = (rgb.1 as f32 * a + buffer[idx + 1] as f32 * inv) as u8;
                        buffer[idx + 2] = (rgb.2 as f32 * a + buffer[idx + 2] as f32 * inv) as u8;
                        buffer[idx + 3] = 255;
                    }
                }
            }
        }
    }

    fn tick_draw(
        &mut self,
        state: &mut EngineState,
        listener: &audio::AudioListener,
        vad_prob: f32,
        vad_label: &TextRasterizer,
        state_label: &TextRasterizer,
        state_color: (u8, u8, u8),
        threshold: f32,
        transcript_lines: &[String],
        scroll_fraction: Option<(f32, f32)>,
        font: &Font,
    ) -> UiBounds {
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

        let transcript_top = wave_top + wave_h + h * 0.02;
        let transcript_bottom = panel_top - h * TEXTBOX_BOTTOM_GAP_FRAC;
        let transcript_h = (transcript_bottom - transcript_top).max(0.0);
        let textbox_x0 = pad;
        let textbox_x1 = w - pad;
        let textbox_y0 = transcript_top;
        let textbox_y1 = transcript_bottom;

        let all_samples = listener.get_samples_by_channel();
        if all_samples.is_empty() {
            return UiBounds::default();
        }
        let samples = &all_samples[0];
        if samples.is_empty() {
            return UiBounds::default();
        }

        // waveform
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
        let points = self.waveform_points.max(2).min(width as usize).min(samples.len());
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
        let mut prev_x = 0.0;
        let mut prev_y = wave_center_y;
        for i in 0..points {
            let amp = self.amplify_nonlinear(smooth[i]).clamp(-1.0, 1.0);
            let x = i as f32 * x_scale;
            let y = wave_center_y - amp * wave_half_amp;
            if i > 0 {
                self.draw_line_thick(buffer, width, height, prev_x, prev_y, x, y, wave_color);
            }
            prev_x = x;
            prev_y = y;
        }

        // low-key transcript textbox
        if transcript_h > 10.0 {
            self.draw_rect(buffer, width, height, textbox_x0, textbox_y0, textbox_x1, textbox_y1, TEXTBOX_BG);
            self.draw_rect(buffer, width, height, textbox_x0, textbox_y0, textbox_x1, textbox_y0 + 1.0, TEXTBOX_BORDER);
            self.draw_rect(buffer, width, height, textbox_x0, textbox_y1 - 1.0, textbox_x1, textbox_y1, TEXTBOX_BORDER);
            self.draw_rect(buffer, width, height, textbox_x0, textbox_y0, textbox_x0 + 1.0, textbox_y1, TEXTBOX_BORDER);
            self.draw_rect(buffer, width, height, textbox_x1 - 1.0, textbox_y0, textbox_x1, textbox_y1, TEXTBOX_BORDER);

            let line_count = transcript_lines.len().max(1);
            let row_h = transcript_h / line_count as f32;
            let text_size = (row_h * 0.46).clamp(12.0, 24.0);
            let text_left = textbox_x0 + (w * 0.012).max(8.0);
            let text_right = textbox_x1 - (w * 0.02).max(12.0);
            let text_w = (text_right - text_left).max(20.0);

            for (i, line) in transcript_lines.iter().enumerate() {
                let mut tr = TextRasterizer::new(font.clone(), text_size);
                tr.set_text(line.to_string());
                tr.tick(text_w, row_h.max(1.0));
                let y = textbox_y0 + row_h * i as f32 + row_h * 0.1;
                self.blend_text(buffer, width, height, &tr, text_left, y, TEXT_COLOR, 0.95);
            }

            if let Some((thumb_top_frac, thumb_h_frac)) = scroll_fraction {
                let track_x0 = textbox_x1 - (w * 0.008).max(6.0);
                let track_x1 = textbox_x1 - (w * 0.004).max(3.0);
                let track_y0 = textbox_y0 + 2.0;
                let track_y1 = textbox_y1 - 2.0;
                self.draw_rect(buffer, width, height, track_x0, track_y0, track_x1, track_y1, TEXTBOX_BORDER);
                let track_h = (track_y1 - track_y0).max(1.0);
                let thumb_h = (track_h * thumb_h_frac.clamp(0.08, 1.0)).max(6.0);
                let thumb_y0 = track_y0 + (track_h - thumb_h) * thumb_top_frac.clamp(0.0, 1.0);
                self.draw_rect(
                    buffer,
                    width,
                    height,
                    track_x0,
                    thumb_y0,
                    track_x1,
                    (thumb_y0 + thumb_h).min(track_y1),
                    TEXTBOX_SCROLL,
                );
            }
        }

        // VAD panel
        self.draw_rect(buffer, width, height, bar_x0, panel_top, bar_x1, panel_top + panel_h, PANEL_BG);
        self.draw_rect(buffer, width, height, bar_x0, bar_y0, bar_x1, bar_y1, WAVE_BASELINE);
        let prob_color = self.lerp_color(vad_prob, WAVE_SILENT, WAVE_SPEECH);
        let fill_x1 = bar_x0 + (bar_x1 - bar_x0) * vad_prob.clamp(0.0, 1.0);
        self.draw_rect(buffer, width, height, bar_x0, bar_y0, fill_x1, bar_y1, prob_color);

        let tx = bar_x0 + (bar_x1 - bar_x0) * threshold.clamp(0.0, 1.0);
        self.draw_rect(buffer, width, height, tx - 1.0, bar_y0 - 2.0, tx + 1.0, bar_y1 + 2.0, (255, 255, 255));

        let slider_x0 = bar_x0;
        let slider_x1 = bar_x1;
        let slider_y0 = panel_top + panel_h * 0.82;
        let slider_y1 = slider_y0 + panel_h * 0.10;
        self.draw_rect(buffer, width, height, slider_x0, slider_y0, slider_x1, slider_y1, WAVE_BASELINE);
        self.draw_rect(
            buffer,
            width,
            height,
            tx - 4.0,
            slider_y0 - panel_h * 0.05,
            tx + 4.0,
            slider_y1 + panel_h * 0.05,
            (235, 235, 238),
        );

        self.blend_text(
            buffer,
            width,
            height,
            vad_label,
            bar_x0,
            panel_top + panel_h * 0.12,
            PANEL_FG,
            1.0,
        );
        self.blend_text(
            buffer,
            width,
            height,
            state_label,
            bar_x1 - w * 0.16,
            panel_top + panel_h * 0.10,
            state_color,
            1.0,
        );

        UiBounds {
            transcript: if transcript_h > 10.0 {
                Some((textbox_x0, textbox_y0, textbox_x1, textbox_y1))
            } else {
                None
            },
            slider: Some((slider_x0, slider_y0 - panel_h * 0.1, slider_x1, slider_y1 + panel_h * 0.1)),
        }
    }
}

pub struct TranscribeApp {
    listener: Option<audio::AudioListener>,
    engine: TranscriptionEngine,
    canvas: VisualCanvas,
    vad_label: TextRasterizer,
    state_label: TextRasterizer,
    text_font: Font,
    vad_prob_ema: f32,
    threshold: f32,
    committed_lines: VecDeque<String>,
    segment_live_text: String,
    speech_run_frames: u32,
    silence_run_frames: u32,
    in_speech_segment: bool,
    transcript_scroll_offset: usize,
    ui_bounds: UiBounds,
    slider_dragging: bool,
}

impl TranscribeApp {
    pub fn new() -> Self {
        let font = fonts::jetbrains_mono();
        let mut vad_label = TextRasterizer::new(font.clone(), 24.0);
        vad_label.set_text("VAD: 0.000".to_string());
        let mut state_label = TextRasterizer::new(font.clone(), 24.0);
        state_label.set_text("SILENCE".to_string());
        Self {
            listener: None,
            engine: TranscriptionEngine::new_with_size_and_backend(None, transcribe_backend_from_env()),
            canvas: VisualCanvas::new(),
            vad_label,
            state_label,
            text_font: font,
            vad_prob_ema: 0.0,
            threshold: THRESHOLD_DEFAULT,
            committed_lines: VecDeque::new(),
            segment_live_text: String::new(),
            speech_run_frames: 0,
            silence_run_frames: 0,
            in_speech_segment: false,
            transcript_scroll_offset: 0,
            ui_bounds: UiBounds::default(),
            slider_dragging: false,
        }
    }

    fn update_threshold_from_mouse(&mut self, state: &EngineState) {
        let Some((x0, _y0, x1, _y1)) = self.ui_bounds.slider else {
            return;
        };
        let t = ((state.mouse.x - x0) / (x1 - x0).max(1.0)).clamp(0.0, 1.0);
        self.threshold = clamp_threshold(t);
    }

    fn pause_input(&self) {
        if let Some(l) = &self.listener {
            let _ = l.pause();
        }
    }
}

impl Drop for TranscribeApp {
    fn drop(&mut self) {
        self.pause_input();
    }
}

impl Application for TranscribeApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        let backend = transcribe_backend_from_env();
        let backend_label = match backend {
            WhisperBackend::Ct2 => "ct2",
            WhisperBackend::Burn => "burn",
        };
        println!(
            "transcribe: backend={backend_label} · visual waveform + rolling transcript + VAD level · Esc to quit"
        );
        let _ = io::stdout().flush();
        println!("transcribe: threshold slider 1..100% (default 30%)");
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
                "transcribe: input {} @ {} Hz",
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
                "transcribe: input {} @ {} Hz",
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
            let l = self.listener.as_ref().expect("checked above");
            let buf = l.buffer();
            (
                l.get_samples_by_channel(),
                buf.sample_rate(),
                buf.ingested_frame_count(),
            )
        };

        self.engine.process_snapshot(sr, &channels, ingested);
        let p_raw = self.engine.last_vad_speech_prob();
        self.vad_prob_ema = self.vad_prob_ema * (1.0 - VAD_PROB_EMA_ALPHA) + p_raw * VAD_PROB_EMA_ALPHA;
        let p = self.vad_prob_ema.clamp(0.0, 1.0);
        let th = self.threshold;
        let seg_start = th;
        let seg_end = (th * 0.80).max(0.01);
        let active = p >= th;

        let live_caption = self.engine.caption().trim();
        if p >= seg_start {
            self.speech_run_frames = self.speech_run_frames.saturating_add(1);
            self.silence_run_frames = 0;
            if self.speech_run_frames >= SPEECH_START_FRAMES {
                self.in_speech_segment = true;
            }
            if self.in_speech_segment && !live_caption.is_empty() {
                self.segment_live_text = live_caption.to_string();
            }
        } else {
            // Segment end uses a lower threshold than start (hysteresis) to avoid rapid toggling.
            if p < seg_end {
                self.silence_run_frames = self.silence_run_frames.saturating_add(1);
            } else {
                self.silence_run_frames = 0;
            }
            self.speech_run_frames = 0;
            if self.in_speech_segment && self.silence_run_frames >= SILENCE_COMMIT_FRAMES {
                let finalized = self.segment_live_text.trim();
                if !finalized.is_empty()
                    && self.committed_lines.back().map(|s| s.as_str()) != Some(finalized)
                {
                    self.committed_lines.push_back(finalized.to_string());
                }
                self.segment_live_text.clear();
                self.in_speech_segment = false;
            }
        }

        while self.committed_lines.len() > 16 {
            let _ = self.committed_lines.pop_front();
        }
        let all_lines: Vec<String> = self.committed_lines.iter().cloned().collect();
        let shape = state.frame.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        let panel_h = (height * LEVEL_PANEL_H_FRAC).max(36.0);
        let panel_top = height - panel_h - height * 0.03;
        let transcript_top = height * (WAVE_HEADROOM_FRAC + WAVE_BAND_FRAC) + height * 0.02;
        let transcript_bottom = panel_top - height * TEXTBOX_BOTTOM_GAP_FRAC;
        let transcript_h = (transcript_bottom - transcript_top).max(0.0);
        let text_size = ((transcript_h / 3.0) * 0.46).clamp(12.0, 24.0);
        let visible_lines = ((transcript_h / (text_size * 1.35)).floor() as usize).max(1);
        let max_offset = all_lines.len().saturating_sub(visible_lines);
        self.transcript_scroll_offset = self.transcript_scroll_offset.min(max_offset);
        let start = all_lines.len().saturating_sub(visible_lines + self.transcript_scroll_offset);
        let end = (start + visible_lines).min(all_lines.len());
        let display_lines = if start < end {
            all_lines[start..end].to_vec()
        } else {
            vec![]
        };
        let scroll_fraction = if all_lines.is_empty() || all_lines.len() <= visible_lines {
            None
        } else {
            let top_frac = if max_offset == 0 {
                0.0
            } else {
                1.0 - (self.transcript_scroll_offset as f32 / max_offset as f32)
            };
            let h_frac = visible_lines as f32 / all_lines.len() as f32;
            Some((top_frac, h_frac))
        };

        self.vad_label
            .set_text(format!("VAD {:>3.0}%   THR {:>3.0}%", p * 100.0, th * 100.0));
        self.state_label
            .set_text(if active { "ACTIVE".to_string() } else { "SILENCE".to_string() });
        let font_size = (height.min(width) * 0.018).clamp(12.0, 18.0);
        self.vad_label.set_font_size(font_size);
        self.vad_label.tick(width, height);
        self.state_label.set_font_size(font_size);
        self.state_label.tick(width, height);

        let l = self.listener.as_ref().expect("checked above");
        self.ui_bounds = self.canvas.tick_draw(
            state,
            l,
            p,
            &self.vad_label,
            &self.state_label,
            if active { STATE_ACTIVE } else { STATE_SILENCE },
            th,
            &display_lines,
            scroll_fraction,
            &self.text_font,
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

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        if let Some((x0, y0, x1, y1)) = self.ui_bounds.slider {
            let mx = state.mouse.x;
            let my = state.mouse.y;
            if mx >= x0 && mx <= x1 && my >= y0 && my <= y1 {
                self.slider_dragging = true;
                self.update_threshold_from_mouse(state);
            }
        }
    }

    fn on_mouse_up(&mut self, _state: &mut EngineState) {
        self.slider_dragging = false;
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        if self.slider_dragging {
            self.update_threshold_from_mouse(state);
        }
    }

    fn on_scroll(&mut self, state: &mut EngineState, _delta_x: f32, delta_y: f32) {
        let Some((x0, y0, x1, y1)) = self.ui_bounds.transcript else {
            return;
        };
        let mx = state.mouse.x;
        let my = state.mouse.y;
        if mx < x0 || mx > x1 || my < y0 || my > y1 {
            return;
        }
        if delta_y.abs() < 0.01 {
            return;
        }
        let total = self.committed_lines.len();
        if total <= 1 {
            return;
        }
        let step = delta_y.signum() as i32;
        let max_offset = total.saturating_sub(1) as i32;
        let next = (self.transcript_scroll_offset as i32 + step).clamp(0, max_offset);
        self.transcript_scroll_offset = next as usize;
    }
}
