use crate::ai::transcription::{
    transcribe_waveform_once_with_language, TranscriptionEngine, WhisperBackend,
};
use crate::apps::text::TranscriptTextView;
use crate::clipboard;
use crate::engine::audio;
use crate::engine::keyboard::shortcuts::ShortcutAction;
use crate::engine::{Application, EngineState};
use crate::rasterizer::fill;
use crate::rasterizer::text::{fonts, text_rasterization::TextRasterizer};
use crate::ui::{
    AudioInputMenuDown, AudioInputSelector, TranscribeLangMenuDown,
    TranscribeLanguageSelector,
};
use crate::ui::onscreen_keyboard::KeyType;
use fontdue::Font;
use std::collections::VecDeque;
use std::io::{self, Write};
use std::time::{Duration, Instant};

const BG: (u8, u8, u8, u8) = (0, 0, 0, 255);
const WAVE_BASELINE: (u8, u8, u8) = (120, 124, 132);
const WAVE_SILENT: (u8, u8, u8) = (230, 230, 235);
const WAVE_SPEECH: (u8, u8, u8) = (90, 230, 140);
const PANEL_BG: (u8, u8, u8) = (25, 28, 36);
const PANEL_FG: (u8, u8, u8) = (210, 214, 220);
const STATE_ACTIVE: (u8, u8, u8) = (92, 230, 142);
const STATE_SILENCE: (u8, u8, u8) = (168, 172, 182);
const TEXTBOX_BORDER: (u8, u8, u8) = (52, 58, 70);
const THRESHOLD_DEFAULT: f32 = 0.30;
const THRESHOLD_MIN: f32 = 0.01;
const THRESHOLD_MAX: f32 = 1.0;
/// Waveform vertical gain (display only): `0` = quiet / low swing, `1` = full (legacy “maxed” look).
const WAVE_DISPLAY_INTENSITY_DEFAULT: f32 = 0.10;
const WAVE_SMOOTH_WINDOW: usize = 7;
const WAVE_SMOOTH_WINDOW_IOS: usize = 5;
const SPEECH_START_FRAMES: u32 = 2;
const SILENCE_COMMIT_FRAMES: u32 = 5;
const SILENCE_CLIP_FRAMES: u32 = 3;
/// Safety cap for a single live decode segment. Prevents unbounded growth when VAD never reaches
/// a commit boundary (common on iOS energy-gated paths under constant background noise).
const SEGMENT_HARD_CLIP_SECONDS_IOS: f32 = 45.0;
const VAD_SEGMENT_EMA_ALPHA: f32 = 0.30;
const DEFAULT_WAVE_POINTS: usize = 640;
const AMPLIFICATION_FACTOR: f32 = 50.0;
const WAVE_BAND_FRAC: f32 = 0.375;
/// When OSK is open: waveform vertical band shrinks (~22% shorter) so transcript + pointer have more room.
const WAVE_BAND_OSK_SCALE: f32 = 0.78;
/// Small gap between transcript bottom and OSK top (px).
const OSK_CONTENT_GAP_PX: f32 = 3.0;
const WAVE_HEADROOM_FRAC: f32 = 0.0;
const LEVEL_PANEL_H_FRAC: f32 = 0.17;
const TEXTBOX_BOTTOM_GAP_FRAC: f32 = 0.015;
const TRANSCRIPT_TEXT_SIZE_MIN: f32 = 26.0;
const TRANSCRIPT_TEXT_SIZE_MAX: f32 = 44.0;
const TRANSCRIPT_INNER_PAD: f32 = 4.0;
const HOLD_TO_RECORD_THRESHOLD: Duration = Duration::from_millis(250);
const DOUBLE_TAP_TIME_MS: u64 = 300;
const DOUBLE_TAP_DISTANCE: f32 = 50.0;
const HOLD_FINAL_TEXT_COLOR: (u8, u8, u8) = (92, 230, 142);

#[derive(Clone, Debug)]
struct TranscriptEntry {
    text: String,
    color: Option<(u8, u8, u8)>,
}

fn transcribe_backend_from_env() -> WhisperBackend {
    std::env::var("XOS_TRANSCRIBE_BACKEND")
        .ok()
        .and_then(|s| WhisperBackend::from_str(&s))
        .unwrap_or(WhisperBackend::Ct2)
}

fn clamp_threshold(v: f32) -> f32 {
    v.clamp(THRESHOLD_MIN, THRESHOLD_MAX)
}

/// Safe area in **pixel** coordinates: `(left, top, width, height)`.
fn safe_layout_pixels(state: &EngineState) -> (f32, f32, f32, f32) {
    let shape = state.frame.shape();
    let w = shape[1] as f32;
    let h = shape[0] as f32;
    let s = &state.frame.safe_region_boundaries;
    (s.x1 * w, s.y1 * h, (s.x2 - s.x1) * w, (s.y2 - s.y1) * h)
}

/// iOS `lib` builds have no `ort` / Silero ONNX, so `last_vad_speech_prob()` is always 0.
/// Use a cheap RMS on recent mono samples so levels / waveform coloring stay responsive.
/// Used by **transcribe** and by [`super::vad::VadApp`].
pub(crate) fn energy_speech_proxy(channels: &[Vec<f32>]) -> f32 {
    if channels.is_empty() || channels[0].is_empty() {
        return 0.0;
    }
    let c = &channels[0];
    let n = c.len();
    if n < 4 {
        return 0.0;
    }
    let take = n.min(2048);
    let s: f32 = c.iter().rev().take(take).map(|v| v * v).sum();
    let rms = (s / take as f32).sqrt();
    (rms * 7.0).min(1.0)
}

#[cfg(not(target_os = "ios"))]
fn input_device_same(a: &audio::AudioDevice, b: &audio::AudioDevice) -> bool {
    if a.name != b.name || a.is_input != b.is_input {
        return false;
    }
    #[cfg(all(
        not(target_arch = "wasm32"),
        any(target_os = "macos", target_os = "windows")
    ))]
    {
        if a.wasapi_loopback != b.wasapi_loopback
            || a.macos_sck_system_audio != b.macos_sck_system_audio
        {
            return false;
        }
    }
    true
}

#[derive(Clone, Copy, Debug, Default)]
struct UiBounds {
    transcript: Option<(f32, f32, f32, f32)>,
    slider: Option<(f32, f32, f32, f32)>,
    /// Full-width “wave intensity” (gain) control below the green capture button.
    intensity: Option<(f32, f32, f32, f32)>,
    /// **lang** button to the right of capture (transcription language; whisper builds only).
    lang: Option<(f32, f32, f32, f32)>,
    /// Dedicated **device** button (left of center mic) that opens the input selector menu.
    device: Option<(f32, f32, f32, f32)>,
    /// Center mic button (tap: toggle live; hold: record-and-run full-clip inference).
    mic: Option<(f32, f32, f32, f32)>,
}

struct VisualCanvas {
    waveform_points: usize,
    wave_smooth_half: usize,
    /// Per-column temporal EMA so the waveform does not stutter when audio chunks arrive in bursts.
    wave_display_ema: Vec<f32>,
}

impl VisualCanvas {
    fn new() -> Self {
        Self {
            #[cfg(target_os = "ios")]
            waveform_points: 420,
            #[cfg(not(target_os = "ios"))]
            waveform_points: DEFAULT_WAVE_POINTS,
            #[cfg(target_os = "ios")]
            wave_smooth_half: WAVE_SMOOTH_WINDOW_IOS / 2,
            #[cfg(not(target_os = "ios"))]
            wave_smooth_half: WAVE_SMOOTH_WINDOW / 2,
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
        vad_raw: f32,
        vad_ema: f32,
        vad_label: &TextRasterizer,
        state_label: &TextRasterizer,
        state_color: (u8, u8, u8),
        threshold: f32,
        waveform_intensity: f32,
        font: &Font,
        wave_rect: (f32, f32, f32, f32),
        safe_layout: (f32, f32, f32, f32),
        audio_selector: &mut AudioInputSelector,
        lang_selector: &mut TranscribeLanguageSelector,
        live_toggle_on: bool,
        ptt_hold_active: bool,
        ptt_hold_seconds: f32,
        show_vad_panel: bool,
        keyboard_top_normalized: Option<f32>,
    ) -> UiBounds {
        let shape = state.frame.shape();
        let width = shape[1] as u32;
        let height = shape[0] as u32;
        let full_w = width as f32;
        let full_h = height as f32;
        let (l, t, sw, sh) = safe_layout;
        let use_full_width_fill = keyboard_top_normalized.is_some();
        let wl = if use_full_width_fill { 0.0 } else { l };
        let ww = if use_full_width_fill { full_w } else { sw };
        let point_cap = (ww as usize).max(2);

        let wave_w = (wave_rect.2 - wave_rect.0).max(0.0);
        let wave_h = (wave_rect.3 - wave_rect.1).max(0.0);
        let btn_s =
            AudioInputSelector::capture_button_size_for_layout(wave_w, wave_h, full_w, full_h);
        #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
        let side_w = TranscribeLanguageSelector::trailing_width_for_layout(btn_s, full_w);
        #[cfg(not(all(feature = "whisper", not(target_arch = "wasm32"))))]
        let side_w = btn_s;
        let (int_stack_r, capture_btn_r, _unused_trailing) =
            AudioInputSelector::layout_intensity_capture_trailing(
                wave_rect.0,
                wave_rect.1,
                wave_rect.2,
                wave_rect.3,
                full_w,
                full_h,
                true,
                0.0,
                AudioInputSelector::TRAILING_TO_CAPTURE_GAP,
            );
        let side_gap = AudioInputSelector::TRAILING_TO_CAPTURE_GAP;
        let mut device_btn_r = (
            capture_btn_r.0 - side_gap - side_w,
            capture_btn_r.1,
            capture_btn_r.0 - side_gap,
            capture_btn_r.3,
        );
        let mut lang_btn_r = (
            capture_btn_r.2 + side_gap,
            capture_btn_r.1,
            capture_btn_r.2 + side_gap + side_w,
            capture_btn_r.3,
        );
        let safe_right = l + sw;
        if device_btn_r.0 < wl {
            let shift = wl - device_btn_r.0;
            device_btn_r.0 += shift;
            device_btn_r.2 += shift;
            lang_btn_r.0 += shift;
            lang_btn_r.2 += shift;
        }
        let layout_right_edge = if use_full_width_fill { full_w } else { safe_right };
        if lang_btn_r.2 > layout_right_edge {
            let shift = lang_btn_r.2 - layout_right_edge;
            device_btn_r.0 -= shift;
            device_btn_r.2 -= shift;
            lang_btn_r.0 -= shift;
            lang_btn_r.2 -= shift;
        }

        let line_top = wave_rect.1;
        let line_bottom = (capture_btn_r.1 - 4.0).max(line_top + 6.0);
        let line_h = (line_bottom - line_top).max(8.0);
        let wave_center_y = line_top + line_h * 0.5;
        let wave_half_amp = line_h * 0.45;
        let pre_gain = 0.02 + 0.98 * waveform_intensity.clamp(0.0, 1.0);

        let panel_h = if show_vad_panel { (sh * LEVEL_PANEL_H_FRAC).max(36.0) } else { 0.0 };
        let control_text_size = ((sh * LEVEL_PANEL_H_FRAC).max(36.0) * 0.42).clamp(12.0, 22.0);
        let panel_top = if show_vad_panel {
            t + sh - panel_h - sh * 0.03
        } else {
            t + sh
        };
        let pad = (sw * 0.03).max(12.0);
        let bar_x0 = l + pad;
        let bar_x1 = l + sw - pad;
        let bar_y0 = panel_top + panel_h * 0.52;
        let bar_y1 = bar_y0 + panel_h * 0.24;

        let transcript_gap_compact = sh * 0.012;
        let transcript_gap_normal = sh * 0.02;
        let (textbox_y0, textbox_y1, textbox_x0, textbox_x1) = if let Some(kty) =
            keyboard_top_normalized
        {
            let transcript_bottom_abs = kty * full_h - OSK_CONTENT_GAP_PX;
            let tt = wave_rect.3 + transcript_gap_compact;
            let tb = transcript_bottom_abs.max(tt + 36.0);
            (tt, tb, 0.0, full_w)
        } else {
            let transcript_top = wave_rect.3 + transcript_gap_normal;
            let transcript_bottom = if show_vad_panel {
                panel_top - sh * TEXTBOX_BOTTOM_GAP_FRAC
            } else {
                t + sh - sh * 0.01
            };
            (
                transcript_top,
                transcript_bottom,
                l + pad,
                l + sw - pad,
            )
        };
        let transcript_h = (textbox_y1 - textbox_y0).max(0.0);

        let all_samples = listener.get_samples_by_channel();
        let samples_empty = all_samples.is_empty() || all_samples[0].is_empty();
        let samples: &[f32] = if samples_empty {
            &[]
        } else {
            &all_samples[0]
        };

        // waveform
        let points = if samples.is_empty() {
            self.waveform_points.max(2).min(point_cap)
        } else {
            self.waveform_points
                .max(2)
                .min(point_cap)
                .min(samples.len())
        };
        let start_idx = if samples.is_empty() {
            0
        } else {
            samples.len().saturating_sub(points)
        };
        let active: &[f32] = if samples.is_empty() {
            &[]
        } else {
            &samples[start_idx..]
        };
        let wave_color = self.lerp_color(vad_raw, WAVE_SILENT, WAVE_SPEECH);
        let x_scale = (ww - 1.0).max(1.0) / (points.saturating_sub(1) as f32).max(1.0);
        let mut smooth = vec![0.0_f32; points];
        let half_w = self.wave_smooth_half;
        if active.is_empty() {
            // No PCM yet (e.g. immediately after switching iOS input)
        } else {
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
        }
        // Decouple the drawn line from buffer chunk boundaries (avoids a “stuttery” look on iOS).
        if self.wave_display_ema.len() != points {
            self.wave_display_ema = vec![0.0_f32; points];
            if !active.is_empty() {
                self.wave_display_ema.copy_from_slice(&smooth);
            }
        } else {
            const WAVE_TEMPORAL_EMA: f32 = 0.42;
            let a = WAVE_TEMPORAL_EMA;
            for i in 0..points {
                self.wave_display_ema[i] = self.wave_display_ema[i] * (1.0 - a) + smooth[i] * a;
            }
        }
        let draw_amp = &self.wave_display_ema;
        let mut prev_x = wl;
        let mut prev_y = wave_center_y;
        {
            let buffer = state.frame_buffer_mut();
            self.draw_line(
                buffer,
                width,
                height,
                wl,
                wave_center_y,
                wl + ww - 1.0,
                wave_center_y,
                WAVE_BASELINE,
            );
            for i in 0..points {
                let amp = self
                    .amplify_nonlinear(draw_amp[i] * pre_gain)
                    .clamp(-1.0, 1.0);
                let x = wl + i as f32 * x_scale;
                let y = wave_center_y - amp * wave_half_amp;
                if i > 0 {
                    self.draw_line_thick(buffer, width, height, prev_x, prev_y, x, y, wave_color);
                }
                prev_x = x;
                prev_y = y;
            }
        }

        audio_selector.draw_with_trailing(state, font, wave_rect, safe_layout, true, 0.0);
        audio_selector.last_button_rect = device_btn_r;

        #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
        if lang_btn_r.2 > lang_btn_r.0 && lang_btn_r.3 > lang_btn_r.1 {
            lang_selector.draw(
                state,
                font,
                lang_btn_r,
                width as usize,
                height as usize,
                safe_layout,
            );
        }

        {
            let buffer = state.frame_buffer_mut();
            if capture_btn_r.2 > capture_btn_r.0 && capture_btn_r.3 > capture_btn_r.1 {
                let bx0 = capture_btn_r.0;
                let by0 = capture_btn_r.1;
                let bx1 = capture_btn_r.2;
                let by1 = capture_btn_r.3;
                let border = if ptt_hold_active {
                    (255, 255, 255)
                } else if live_toggle_on {
                    (0, 255, 0)
                } else {
                    (100, 100, 100)
                };
                let fill_col = if ptt_hold_active {
                    (255, 255, 255)
                } else if live_toggle_on {
                    (0, 255, 0)
                } else {
                    (18, 20, 28)
                };
                self.draw_rect(buffer, width, height, bx0, by0, bx1, by1, fill_col);
                self.draw_rect(buffer, width, height, bx0, by0, bx1, by0 + 3.0, border);
                self.draw_rect(buffer, width, height, bx0, by1 - 3.0, bx1, by1, border);
                self.draw_rect(buffer, width, height, bx0, by0, bx0 + 3.0, by1, border);
                self.draw_rect(buffer, width, height, bx1 - 3.0, by0, bx1, by1, border);
            }
            if device_btn_r.2 > device_btn_r.0 && device_btn_r.3 > device_btn_r.1 {
                self.draw_rect(
                    buffer,
                    width,
                    height,
                    device_btn_r.0,
                    device_btn_r.1,
                    device_btn_r.2,
                    device_btn_r.3,
                    (32, 36, 44),
                );
                self.draw_rect(
                    buffer,
                    width,
                    height,
                    device_btn_r.0,
                    device_btn_r.1,
                    device_btn_r.2,
                    device_btn_r.1 + 3.0,
                    (180, 186, 196),
                );
                self.draw_rect(
                    buffer,
                    width,
                    height,
                    device_btn_r.0,
                    device_btn_r.3 - 3.0,
                    device_btn_r.2,
                    device_btn_r.3,
                    (180, 186, 196),
                );
                self.draw_rect(
                    buffer,
                    width,
                    height,
                    device_btn_r.0,
                    device_btn_r.1,
                    device_btn_r.0 + 3.0,
                    device_btn_r.3,
                    (180, 186, 196),
                );
                self.draw_rect(
                    buffer,
                    width,
                    height,
                    device_btn_r.2 - 3.0,
                    device_btn_r.1,
                    device_btn_r.2,
                    device_btn_r.3,
                    (180, 186, 196),
                );
                let mut dev_text = TextRasterizer::new(font.clone(), ((device_btn_r.3 - device_btn_r.1) * 0.28).clamp(10.0, 20.0));
                dev_text.set_text("device".to_string());
                dev_text.tick(width as f32, height as f32);
                self.blend_text(
                    buffer,
                    width,
                    height,
                    &dev_text,
                    device_btn_r.0 + (device_btn_r.2 - device_btn_r.0) * 0.14,
                    device_btn_r.1 + (device_btn_r.3 - device_btn_r.1) * 0.24,
                    (235, 238, 242),
                    1.0,
                );
            }
            if int_stack_r.2 > int_stack_r.0 && int_stack_r.3 > int_stack_r.1 {
                let track_inset = (int_stack_r.3 - int_stack_r.1) * 0.18;
                let tr_y0 = int_stack_r.1 + track_inset;
                let tr_y1 = int_stack_r.3 - track_inset;
                let t = waveform_intensity.clamp(0.0, 1.0);
                self.draw_rect(
                    buffer,
                    width,
                    height,
                    int_stack_r.0,
                    tr_y0,
                    int_stack_r.2,
                    tr_y1,
                    (42, 46, 56),
                );
                let w = (int_stack_r.2 - int_stack_r.0).max(0.0);
                let fill_x1 = int_stack_r.0 + w * t;
                self.draw_rect(
                    buffer,
                    width,
                    height,
                    int_stack_r.0,
                    tr_y0,
                    fill_x1,
                    tr_y1,
                    (70, 160, 100),
                );
                let tx = int_stack_r.0 + w * t;
                self.draw_rect(
                    buffer,
                    width,
                    height,
                    tx - 1.0,
                    int_stack_r.1,
                    tx + 1.0,
                    int_stack_r.3,
                    (255, 255, 255),
                );
            }
        }

        let buffer = state.frame_buffer_mut();

        // Transcript area: background + border only; text is drawn by [`TranscriptTextView`] (TextApp-style scroll).
        if transcript_h > 10.0 {
            // No fill: transcript sits on the same black as the rest of the frame; outline only.
            self.draw_rect(buffer, width, height, textbox_x0, textbox_y0, textbox_x1, textbox_y0 + 1.0, TEXTBOX_BORDER);
            self.draw_rect(buffer, width, height, textbox_x0, textbox_y1 - 1.0, textbox_x1, textbox_y1, TEXTBOX_BORDER);
            self.draw_rect(buffer, width, height, textbox_x0, textbox_y0, textbox_x0 + 1.0, textbox_y1, TEXTBOX_BORDER);
            self.draw_rect(buffer, width, height, textbox_x1 - 1.0, textbox_y0, textbox_x1, textbox_y1, TEXTBOX_BORDER);
        }
        if ptt_hold_active {
            let mut hold_timer = TextRasterizer::new(font.clone(), control_text_size);
            hold_timer.set_text(format!("HOLD {:.1}s", ptt_hold_seconds.max(0.0)));
            hold_timer.tick(width as f32, height as f32);
            self.blend_text(
                buffer,
                width,
                height,
                &hold_timer,
                capture_btn_r.0,
                (capture_btn_r.1 - control_text_size * 1.6).max(0.0),
                (255, 255, 255),
                1.0,
            );
        }
        let tx = bar_x0 + (bar_x1 - bar_x0) * threshold.clamp(0.0, 1.0);
        let slider_x0 = bar_x0;
        let slider_x1 = bar_x1;
        let slider_y0 = panel_top + panel_h * 0.82;
        let slider_y1 = slider_y0 + panel_h * 0.10;
        if show_vad_panel {
            self.draw_rect(buffer, width, height, bar_x0, panel_top, bar_x1, panel_top + panel_h, PANEL_BG);
            self.draw_rect(buffer, width, height, bar_x0, bar_y0, bar_x1, bar_y1, WAVE_BASELINE);
            let raw_color = self.lerp_color(vad_raw, WAVE_SILENT, WAVE_SPEECH);
            let ema_color = (130, 190, 255);
            let raw_fill_x1 = bar_x0 + (bar_x1 - bar_x0) * vad_raw.clamp(0.0, 1.0);
            let ema_fill_x1 = bar_x0 + (bar_x1 - bar_x0) * vad_ema.clamp(0.0, 1.0);
            let mid_y = bar_y0 + (bar_y1 - bar_y0) * 0.5;
            self.draw_rect(buffer, width, height, bar_x0, bar_y0, raw_fill_x1, mid_y, raw_color);
            self.draw_rect(buffer, width, height, bar_x0, mid_y, ema_fill_x1, bar_y1, ema_color);
            self.draw_rect(buffer, width, height, tx - 1.0, bar_y0 - 2.0, tx + 1.0, bar_y1 + 2.0, (255, 255, 255));
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
                l + sw - sw * 0.16,
                panel_top + panel_h * 0.10,
                state_color,
                1.0,
            );
        }

        let intensity_hit = (int_stack_r.0, int_stack_r.1 - 6.0, int_stack_r.2, int_stack_r.3 + 5.0);
        #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
        let lang_hit = (lang_btn_r.0, lang_btn_r.1 - 4.0, lang_btn_r.2, lang_btn_r.3 + 4.0);
        UiBounds {
            transcript: if transcript_h > 10.0 {
                Some((textbox_x0, textbox_y0, textbox_x1, textbox_y1))
            } else {
                None
            },
            slider: if show_vad_panel {
                Some((slider_x0, slider_y0 - panel_h * 0.1, slider_x1, slider_y1 + panel_h * 0.1))
            } else {
                None
            },
            intensity: if int_stack_r.2 > int_stack_r.0 {
                Some(intensity_hit)
            } else {
                None
            },
            lang: {
                #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
                {
                    if lang_btn_r.2 > lang_btn_r.0 {
                        Some(lang_hit)
                    } else {
                        None
                    }
                }
                #[cfg(not(all(feature = "whisper", not(target_arch = "wasm32"))))]
                {
                    None
                }
            },
            device: if device_btn_r.2 > device_btn_r.0 {
                Some(device_btn_r)
            } else {
                None
            },
            mic: if capture_btn_r.2 > capture_btn_r.0 {
                Some(capture_btn_r)
            } else {
                None
            },
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
    transcript_view: TranscriptTextView,
    threshold: f32,
    vad_prob_seg_ema: f32,
    vad_prob_visual_ema: f32,
    committed_lines: VecDeque<TranscriptEntry>,
    segment_live_text: String,
    speech_run_frames: u32,
    silence_run_frames: u32,
    silence_idle_clip_frames: u32,
    in_speech_segment: bool,
    ui_bounds: UiBounds,
    slider_dragging: bool,
    /// Waveform display gain 0..1 (does not affect transcription, only the drawn line).
    waveform_intensity: f32,
    intensity_slider_dragging: bool,
    /// Left button down in transcript; drag scrolls (same as standalone text).
    transcript_pointer_down: bool,
    audio_selector: AudioInputSelector,
    /// Transcription language (`en` / `ja`); no-op on builds without the whisper feature.
    lang_selector: TranscribeLanguageSelector,
    live_transcribe_enabled: bool,
    mic_pointer_down: bool,
    mic_pointer_down_at: Option<Instant>,
    ptt_hold_active: bool,
    ptt_hold_started_at: Option<Instant>,
    ptt_hold_pcm: Vec<f32>,
    ptt_hold_last_ingested: Option<u64>,
    ptt_hold_sample_rate: u32,
    font_version: u64,
    last_transcript_tap_time: Option<Instant>,
    last_transcript_tap_x: f32,
    last_transcript_tap_y: f32,
    transcript_tap_scrolled: bool,
    transcript_touch_started_on_keyboard: bool,
}

impl TranscribeApp {
    pub fn new() -> Self {
        let font = fonts::default_font();
        let mut vad_label = TextRasterizer::new(font.clone(), 24.0);
        vad_label.set_text("VAD: 0.000".to_string());
        let mut state_label = TextRasterizer::new(font.clone(), 24.0);
        state_label.set_text("SILENCE".to_string());
        let transcript_view = TranscriptTextView::new(font.clone(), 30.0);
        let engine = match TranscriptionEngine::new_with_size_backend_language(
            None,
            transcribe_backend_from_env(),
            Some("en"),
        ) {
            Ok(e) => e,
            Err(_) => TranscriptionEngine::new_with_size_and_backend(None, transcribe_backend_from_env()),
        };
        Self {
            listener: None,
            engine,
            canvas: VisualCanvas::new(),
            vad_label,
            state_label,
            text_font: font,
            transcript_view,
            threshold: THRESHOLD_DEFAULT,
            vad_prob_seg_ema: 0.0,
            vad_prob_visual_ema: 0.0,
            committed_lines: VecDeque::new(),
            segment_live_text: String::new(),
            speech_run_frames: 0,
            silence_run_frames: 0,
            silence_idle_clip_frames: 0,
            in_speech_segment: false,
            ui_bounds: UiBounds::default(),
            slider_dragging: false,
            waveform_intensity: WAVE_DISPLAY_INTENSITY_DEFAULT,
            intensity_slider_dragging: false,
            transcript_pointer_down: false,
            audio_selector: AudioInputSelector::new(),
            lang_selector: TranscribeLanguageSelector::new(),
            live_transcribe_enabled: false,
            mic_pointer_down: false,
            mic_pointer_down_at: None,
            ptt_hold_active: false,
            ptt_hold_started_at: None,
            ptt_hold_pcm: Vec::new(),
            ptt_hold_last_ingested: None,
            ptt_hold_sample_rate: 16_000,
            font_version: fonts::default_font_version(),
            last_transcript_tap_time: None,
            last_transcript_tap_x: 0.0,
            last_transcript_tap_y: 0.0,
            transcript_tap_scrolled: false,
            transcript_touch_started_on_keyboard: false,
        }
    }

    fn refresh_fonts_if_needed(&mut self) {
        let current_version = fonts::default_font_version();
        if current_version == self.font_version {
            return;
        }
        self.font_version = current_version;

        let new_font = fonts::default_font();

        let vad_size = self.vad_label.font_size;
        let vad_text = self.vad_label.text.clone();
        self.vad_label = TextRasterizer::new(new_font.clone(), vad_size);
        self.vad_label.set_text(vad_text);

        let state_size = self.state_label.font_size;
        let state_text = self.state_label.text.clone();
        self.state_label = TextRasterizer::new(new_font.clone(), state_size);
        self.state_label.set_text(state_text);

        self.text_font = new_font.clone();
        self.transcript_view.set_font(new_font);
    }

    /// Replace the live Whisper decode thread with the current [`TranscribeLanguageSelector`] code.
    fn recreate_transcription_engine(&mut self) {
        #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
        {
            let code = self.lang_selector.current_language_code();
            let backend = transcribe_backend_from_env();
            match TranscriptionEngine::new_with_size_backend_language(None, backend, Some(code)) {
                Ok(e) => {
                    self.engine = e;
                    if let Some(l) = &self.listener {
                        self.engine
                            .set_device_hint(l.device_name(), l.buffer().sample_rate());
                    }
                    self.committed_lines.clear();
                    self.segment_live_text.clear();
                    self.in_speech_segment = false;
                    self.speech_run_frames = 0;
                    self.silence_run_frames = 0;
                    self.silence_idle_clip_frames = 0;
                    self.transcript_view.set_text(String::new());
                }
                Err(e) => eprintln!("transcribe: failed to set language: {e}"),
            }
        }
    }

    fn recreate_input_listener(&mut self) -> Result<(), String> {
        let Some(device) = self.audio_selector.resolved_input_device() else {
            return Err("No input device selected".to_string());
        };
        if let Some(old) = self.listener.take() {
            let _ = old.pause();
            drop(old);
        }
        // Keep capture buffering aligned with iOS so live decode cadence/latency characteristics
        // match across platforms.
        let buffer_duration = 3.0_f32;

        let listener = audio::AudioListener::new(&device, buffer_duration)?;
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

    fn update_threshold_from_mouse(&mut self, state: &EngineState) {
        let Some((x0, _y0, x1, _y1)) = self.ui_bounds.slider else {
            return;
        };
        let t = ((state.mouse.x - x0) / (x1 - x0).max(1.0)).clamp(0.0, 1.0);
        self.threshold = clamp_threshold(t);
    }

    fn update_waveform_intensity_from_mouse(&mut self, state: &EngineState) {
        let Some((x0, _y0, x1, _y1)) = self.ui_bounds.intensity else {
            return;
        };
        let t = ((state.mouse.x - x0) / (x1 - x0).max(1.0)).clamp(0.0, 1.0);
        self.waveform_intensity = t;
    }

    fn pause_input(&self) {
        if let Some(l) = &self.listener {
            let _ = l.pause();
        }
    }

    fn normalize_text(s: &str) -> String {
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn push_dedup_committed_line(&mut self, line: &str, color: Option<(u8, u8, u8)>) {
        let t = Self::normalize_text(line);
        if t.is_empty() {
            return;
        }
        if self.committed_lines.back().map(|e| e.text.as_str()) != Some(t.as_str()) {
            self.committed_lines
                .push_back(TranscriptEntry { text: t, color });
        }
    }

    fn drain_engine_commits_to_transcript(&mut self) {
        for line in self.engine.drain_stdout_commits() {
            self.push_dedup_committed_line(&line, None);
        }
    }

    fn transcript_text_size(height: f32, f3_ui_scale_mul: f32) -> f32 {
        let base = (height * 0.039).clamp(TRANSCRIPT_TEXT_SIZE_MIN, TRANSCRIPT_TEXT_SIZE_MAX);
        (base * f3_ui_scale_mul.clamp(0.25, 5.0)).clamp(TRANSCRIPT_TEXT_SIZE_MIN, TRANSCRIPT_TEXT_SIZE_MAX * 2.0)
    }

    fn handle_transcript_action_key(&mut self, action: KeyType) {
        if let Some(copied) = self.transcript_view.on_action_key(action) {
            if let Err(e) = clipboard::set_contents(&copied) {
                #[cfg(not(target_arch = "wasm32"))]
                eprintln!(
                    "transcript: failed to write clipboard ({e}). On Linux install wl-clipboard or xclip."
                );
            }
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
        println!("transcribe: backend={backend_label} · tap mic to toggle live, hold mic for full-clip inference, tap device/lang for menus · Esc to quit");
        let _ = io::stdout().flush();
        println!("transcribe: threshold slider 1..100% (default 30%)");
        let _ = io::stdout().flush();

        self.audio_selector.refresh_inputs_from_system();
        if self.audio_selector.input_devices.is_empty() {
            return Err(
                "No audio input devices found. On Windows, choose “… (system audio)” for built-in \
                 capture. Otherwise use a mic or a loopback driver (e.g. BlackHole on macOS)."
                    .to_string(),
            );
        }

        #[cfg(target_os = "ios")]
        {
            self.audio_selector.use_default_input = false;
            self.audio_selector.input_device_index = audio::default_input()
                .and_then(|d| {
                    self.audio_selector
                        .input_devices
                        .iter()
                        .position(|x| x.device_id == d.device_id)
                })
                .unwrap_or(0);
        }

        #[cfg(not(target_os = "ios"))]
        {
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            let preferred = audio::preferred_system_audio_input_device();
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            let preferred: Option<audio::AudioDevice> = None;

            let initial = preferred
                .or_else(|| {
                    self.audio_selector.input_devices.iter().find(|d| {
                        matches!(
                            d.input_kind_hint(),
                            Some(audio::InputDeviceKind::LoopbackOrVirtual)
                        )
                    }).cloned()
                })
                .or_else(|| self.audio_selector.input_devices.first().cloned())
                .ok_or_else(|| {
                    "No usable audio input. On macOS, grant Screen Recording for system audio; on \
                     Windows use a “(system audio)” capture device; on Linux use a virtual cable \
                     (e.g. PulseAudio monitor / PipeWire loopback)."
                        .to_string()
                })?;

            let def = audio::default_input();
            if def.as_ref().is_some_and(|d| input_device_same(d, &initial)) {
                self.audio_selector.use_default_input = true;
            } else {
                self.audio_selector.use_default_input = false;
                self.audio_selector.input_device_index = self
                    .audio_selector
                    .input_devices
                    .iter()
                    .position(|d| input_device_same(d, &initial))
                    .unwrap_or(0);
            }
        }

        self.recreate_input_listener()?;

        if self.audio_selector.input_devices.len() > 1 {
            println!(
                "transcribe: tap device to choose input (Default or a device)"
            );
            let _ = io::stdout().flush();
        }
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        while state.keyboard.onscreen.pop_pending_char().is_some() {}
        if let Some(action) = state.keyboard.onscreen.get_last_action_key() {
            self.handle_transcript_action_key(action);
        }
        self.refresh_fonts_if_needed();
        fill(&mut state.frame, BG);

        if self.listener.is_none() {
            return;
        }

        self.lang_selector.tick_hold_opens_menu();
        if self.mic_pointer_down
            && !self.ptt_hold_active
            && self
                .mic_pointer_down_at
                .is_some_and(|t| t.elapsed() >= HOLD_TO_RECORD_THRESHOLD)
        {
            self.ptt_hold_active = true;
            self.ptt_hold_started_at = Some(Instant::now());
            self.ptt_hold_pcm.clear();
            self.ptt_hold_last_ingested = None;
            if let Some(l) = &self.listener {
                self.ptt_hold_sample_rate = l.buffer().sample_rate();
            }
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

        if self.live_transcribe_enabled && !self.ptt_hold_active {
            self.engine.process_snapshot(sr, &channels, ingested);
        }
        if self.ptt_hold_active {
            let mono = if channels.is_empty() {
                Vec::new()
            } else {
                channels[0].clone()
            };
            if let Some(prev) = self.ptt_hold_last_ingested {
                if ingested > prev && !mono.is_empty() {
                    let delta = (ingested - prev) as usize;
                    let take = delta.min(mono.len());
                    if take > 0 {
                        self.ptt_hold_pcm
                            .extend_from_slice(&mono[mono.len().saturating_sub(take)..]);
                    }
                }
            } else {
                // Start from hold-begin watermark so the one-shot pass receives only audio
                // captured during this press, not pre-existing ring-buffer samples.
                self.ptt_hold_last_ingested = Some(ingested);
            }
            self.ptt_hold_last_ingested = Some(ingested);
        }
        let buffered_secs = if self.live_transcribe_enabled {
            self.engine.buffered_segment_seconds()
        } else {
            0.0
        };
        // Use the same anti-growth clip window as iOS for cross-platform parity.
        let hard_clip_seconds = SEGMENT_HARD_CLIP_SECONDS_IOS;
        if buffered_secs >= hard_clip_seconds {
            // Force-commit + clip even if silence gating didn't trigger, so segment PCM cannot grow forever.
            self.engine.flush_live_to_stdout_commits();
            self.drain_engine_commits_to_transcript();
            self.engine.clip_consumed_audio_cursor();
            self.segment_live_text.clear();
            self.in_speech_segment = false;
            self.speech_run_frames = 0;
            self.silence_run_frames = 0;
            self.silence_idle_clip_frames = 0;
        }
        let p_engine = self.engine.last_vad_speech_prob().clamp(0.0, 1.0);
        let energy = energy_speech_proxy(&channels);
        // iOS parity for all targets: combine model VAD with energy proxy so speech gating remains
        // responsive even when platform VAD confidence is unavailable or conservative.
        let p = p_engine.max(energy);
        self.vad_prob_visual_ema = self.vad_prob_visual_ema * 0.82 + p * 0.18;
        self.vad_prob_seg_ema =
            self.vad_prob_seg_ema * (1.0 - VAD_SEGMENT_EMA_ALPHA) + p * VAD_SEGMENT_EMA_ALPHA;
        let th = self.threshold;
        let seg_end = (th * 0.80).max(0.01);
        let speech_now = p >= th || self.vad_prob_seg_ema >= seg_end;
        let active = speech_now;

        let live_caption = if self.live_transcribe_enabled {
            self.engine.caption().trim().to_string()
        } else {
            String::new()
        };
        if speech_now {
            self.speech_run_frames = self.speech_run_frames.saturating_add(1);
            self.silence_run_frames = 0;
            self.silence_idle_clip_frames = 0;
            if self.speech_run_frames >= SPEECH_START_FRAMES {
                self.in_speech_segment = true;
            }
            if self.in_speech_segment && !live_caption.is_empty() {
                if live_caption.len() >= self.segment_live_text.len() {
                    self.segment_live_text = live_caption.clone();
                }
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
                // Force one final emit from engine so we commit the best available end-of-utterance text.
                self.engine.flush_live_to_stdout_commits();
                let mut finalized = String::new();
                for line in self.engine.drain_stdout_commits() {
                    let t = Self::normalize_text(&line);
                    if !t.is_empty() {
                        finalized = t;
                    }
                }
                if finalized.is_empty() {
                    finalized = Self::normalize_text(&self.segment_live_text);
                }
                let live_norm = Self::normalize_text(&self.segment_live_text);
                if !live_norm.is_empty() && live_norm.len() > finalized.len() {
                    finalized = live_norm;
                }
                self.push_dedup_committed_line(&finalized, None);
                self.segment_live_text.clear();
                self.in_speech_segment = false;
                // Critical: clip old audio out of the decode segment so we don't reprocess it.
                self.engine.clip_consumed_audio_cursor();
                self.silence_idle_clip_frames = 0;
            }
            if !self.in_speech_segment {
                self.silence_idle_clip_frames = self.silence_idle_clip_frames.saturating_add(1);
                if self.silence_idle_clip_frames >= SILENCE_CLIP_FRAMES {
                    // Don't keep growing silent buffers; keep cursor near "now" while idle.
                    self.engine.clip_consumed_audio_cursor();
                    self.silence_idle_clip_frames = 0;
                }
            }
        }

        while self.committed_lines.len() > 16 {
            let _ = self.committed_lines.pop_front();
        }
        let mut full_text = String::new();
        let mut color_spans: Vec<(usize, usize, (u8, u8, u8))> = Vec::new();
        let mut char_cursor = 0usize;
        for (i, entry) in self.committed_lines.iter().enumerate() {
            if i > 0 {
                full_text.push('\n');
                char_cursor += 1;
            }
            let start = char_cursor;
            full_text.push_str(&entry.text);
            char_cursor += entry.text.chars().count();
            if let Some(color) = entry.color {
                color_spans.push((start, char_cursor, color));
            }
        }
        if self.in_speech_segment {
            let live = Self::normalize_text(&live_caption);
            if !live.is_empty() {
                if !full_text.is_empty() {
                    full_text.push('\n');
                }
                full_text.push_str(&live);
            }
        }

        let shape = state.frame.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        let safe = safe_layout_pixels(state);
        let (sl, st, ssw, ssh) = safe;
        self.vad_label.set_text(format!(
            "RAW {:>3.0}%  EMA {:>3.0}%  THR {:>3.0}%",
            p * 100.0,
            self.vad_prob_visual_ema * 100.0,
            th * 100.0
        ));
        self.state_label.set_text(format!(
            "{:.2}s {}",
            self.engine.buffered_segment_seconds(),
            if active { "ACTIVE" } else { "SILENCE" }
        ));
        let font_size = (ssh.min(ssw) * 0.02).clamp(12.0, 20.0);
        self.vad_label.set_font_size(font_size);
        self.vad_label.tick(width, height);
        self.state_label.set_font_size(font_size);
        self.state_label.tick(width, height);

        let listener = self.listener.as_ref().expect("checked above");
        let h = height;
        let w = width;
        let (_, keyboard_top_norm, _, _) = state.keyboard.onscreen.top_edge_coordinates();
        let osk_shown = state.keyboard.onscreen.is_shown();
        let wave_top = st + ssh * WAVE_HEADROOM_FRAC;
        let wave_h = if osk_shown {
            ssh * WAVE_BAND_FRAC * WAVE_BAND_OSK_SCALE
        } else {
            ssh * WAVE_BAND_FRAC
        };
        let wave_rect = if osk_shown {
            (0.0, wave_top, w, wave_top + wave_h)
        } else {
            (sl, wave_top, sl + ssw, wave_top + wave_h)
        };

        let hold_seconds = self
            .ptt_hold_started_at
            .map(|t| t.elapsed().as_secs_f32())
            .unwrap_or(0.0);
        self.ui_bounds = self.canvas.tick_draw(
            state,
            listener,
            p,
            self.vad_prob_visual_ema,
            &self.vad_label,
            &self.state_label,
            if active { STATE_ACTIVE } else { STATE_SILENCE },
            th,
            self.waveform_intensity,
            &self.text_font,
            wave_rect,
            safe,
            &mut self.audio_selector,
            &mut self.lang_selector,
            self.live_transcribe_enabled,
            self.ptt_hold_active,
            hold_seconds,
            !state.keyboard.onscreen.is_shown(),
            if osk_shown {
                Some(keyboard_top_norm)
            } else {
                None
            },
        );

        if self.ui_bounds.transcript.is_some() {
            let pad = (ssw * 0.03).max(12.0);
            let (transcript_top, transcript_bottom, textbox_x0, textbox_x1) = if osk_shown {
                let tt_osk = wave_rect.3 + ssh * 0.012;
                let tb =
                    (keyboard_top_norm * h - OSK_CONTENT_GAP_PX).max(tt_osk + 36.0);
                (tt_osk, tb, 0.0, w)
            } else {
                let transcript_top = wave_top + wave_h + ssh * 0.02;
                let transcript_bottom = {
                    let panel_h = (ssh * LEVEL_PANEL_H_FRAC).max(36.0);
                    let panel_top = st + ssh - panel_h - ssh * 0.03;
                    panel_top - ssh * TEXTBOX_BOTTOM_GAP_FRAC
                };
                (transcript_top, transcript_bottom, sl + pad, sl + ssw - pad)
            };
            let clip_x0 = textbox_x0 + 1.0;
            let clip_y0 = transcript_top + TRANSCRIPT_INNER_PAD;
            let clip_x1 = (textbox_x1 - 1.0).min(w);
            let clip_y1 = (transcript_bottom - TRANSCRIPT_INNER_PAD).min(h);
            self.transcript_view
                .set_font_size(Self::transcript_text_size(ssh, state.f3_ui_scale_multiplier()));
            self.transcript_view
                .set_text_with_color_spans(full_text, color_spans);
            self.transcript_view
                .tick(state, (clip_x0, clip_y0, clip_x1, clip_y1));
        }
    }

    fn on_key_char(&mut self, _state: &mut EngineState, ch: char) {
        if ch == '\u{1b}' {
            self.pause_input();
            #[cfg(not(target_arch = "wasm32"))]
            crate::engine::native_engine::request_exit();
        }
    }

    fn on_key_shortcut(&mut self, _state: &mut EngineState, shortcut: ShortcutAction) {
        match shortcut {
            ShortcutAction::Copy => self.handle_transcript_action_key(KeyType::Copy),
            ShortcutAction::SelectAll => self.handle_transcript_action_key(KeyType::SelectAll),
            ShortcutAction::Cut | ShortcutAction::Paste | ShortcutAction::Undo | ShortcutAction::Redo => {
            }
        }
    }

    fn prepare_shutdown(&mut self, _state: &mut EngineState) {
        if let Some(listener) = self.listener.take() {
            let _ = listener.pause();
        }
        _state.keyboard.onscreen.set_read_only_mode(false);
        if _state.keyboard.onscreen.is_shown() {
            _state.keyboard.onscreen.hide();
        }
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        let shape = state.frame.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        if state
            .keyboard
            .onscreen
            .on_mouse_down(state.mouse.x, state.mouse.y, width, height)
        {
            self.transcript_touch_started_on_keyboard = true;
            return;
        }
        self.transcript_touch_started_on_keyboard = false;
        if state.keyboard.onscreen.is_trackpad_mode() {
            let (_, keyboard_top_y, _, _) = state.keyboard.onscreen.top_edge_coordinates();
            let keyboard_top_px = keyboard_top_y * height;
            if state.mouse.y >= keyboard_top_px {
                if let Some(r) = self.ui_bounds.transcript {
                    self.transcript_view.on_trackpad_pointer_down(
                        state.mouse.x,
                        state.mouse.y,
                        r,
                    );
                }
                return;
            }
        }
        if self.audio_selector.show_menu {
            self.lang_selector.show_menu = false;
            let layout = safe_layout_pixels(state);
            match self
                .audio_selector
                .on_menu_pointer_down(state.mouse.x, state.mouse.y, layout)
            {
                AudioInputMenuDown::Dismiss | AudioInputMenuDown::DismissInColumn => {
                    self.audio_selector.show_menu = false;
                }
                AudioInputMenuDown::Pick { selection_changed } => {
                    if selection_changed {
                        if let Err(e) = self.recreate_input_listener() {
                            eprintln!("transcribe: failed to switch input: {e}");
                        }
                    }
                }
            }
            self.transcript_pointer_down = false;
            self.slider_dragging = false;
            self.intensity_slider_dragging = false;
            return;
        }
        #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
        if self.lang_selector.show_menu {
            self.audio_selector.show_menu = false;
            let layout = safe_layout_pixels(state);
            match self.lang_selector.on_menu_pointer_down(
                state.mouse.x,
                state.mouse.y,
                layout,
            ) {
                TranscribeLangMenuDown::Dismiss | TranscribeLangMenuDown::DismissInColumn => {
                    self.lang_selector.show_menu = false;
                }
                TranscribeLangMenuDown::Pick { changed, .. } => {
                    if changed {
                        self.recreate_transcription_engine();
                    }
                    self.lang_selector.show_menu = false;
                }
            }
            self.transcript_pointer_down = false;
            self.slider_dragging = false;
            self.intensity_slider_dragging = false;
            return;
        }
        if let Some((x0, y0, x1, y1)) = self.ui_bounds.intensity {
            let mx = state.mouse.x;
            let my = state.mouse.y;
            if mx >= x0 && mx <= x1 && my >= y0 && my <= y1 {
                self.lang_selector.show_menu = false;
                self.intensity_slider_dragging = true;
                self.update_waveform_intensity_from_mouse(state);
                self.transcript_pointer_down = false;
                self.slider_dragging = false;
                return;
            }
        }
        #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
        if self
            .lang_selector
            .on_button_pointer_down(state.mouse.x, state.mouse.y)
        {
            self.audio_selector.show_menu = false;
            self.transcript_pointer_down = false;
            self.slider_dragging = false;
            self.intensity_slider_dragging = false;
            return;
        }
        if self
            .ui_bounds
            .device
            .is_some_and(|r| state.mouse.x >= r.0 && state.mouse.x <= r.2 && state.mouse.y >= r.1 && state.mouse.y <= r.3)
        {
            self.audio_selector.show_menu = true;
            self.lang_selector.show_menu = false;
            self.transcript_pointer_down = false;
            self.slider_dragging = false;
            self.intensity_slider_dragging = false;
            return;
        }
        if self
            .ui_bounds
            .mic
            .is_some_and(|r| state.mouse.x >= r.0 && state.mouse.x <= r.2 && state.mouse.y >= r.1 && state.mouse.y <= r.3)
        {
            self.mic_pointer_down = true;
            self.mic_pointer_down_at = Some(Instant::now());
            self.lang_selector.show_menu = false;
            self.audio_selector.show_menu = false;
            self.transcript_pointer_down = false;
            self.slider_dragging = false;
            self.intensity_slider_dragging = false;
            return;
        }
        if let Some((x0, y0, x1, y1)) = self.ui_bounds.slider {
            let mx = state.mouse.x;
            let my = state.mouse.y;
            if mx >= x0 && mx <= x1 && my >= y0 && my <= y1 {
                self.lang_selector.show_menu = false;
                self.slider_dragging = true;
                self.update_threshold_from_mouse(state);
                self.transcript_pointer_down = false;
                self.intensity_slider_dragging = false;
                return;
            }
        }
        if let Some((x0, y0, x1, y1)) = self.ui_bounds.transcript {
            let mx = state.mouse.x;
            let my = state.mouse.y;
            if mx >= x0 && mx <= x1 && my >= y0 && my <= y1 {
                let now = Instant::now();
                let is_double_tap = self.last_transcript_tap_time.is_some_and(|last_time| {
                    let time_since_last = now.duration_since(last_time);
                    let dist = ((mx - self.last_transcript_tap_x).powi(2)
                        + (my - self.last_transcript_tap_y).powi(2))
                    .sqrt();
                    time_since_last < Duration::from_millis(DOUBLE_TAP_TIME_MS)
                        && dist < DOUBLE_TAP_DISTANCE
                        && !self.transcript_tap_scrolled
                });
                if is_double_tap {
                    state.keyboard.onscreen.set_read_only_mode(true);
                    state.keyboard.onscreen.toggle_minimize();
                    self.last_transcript_tap_time = None;
                    self.transcript_tap_scrolled = false;
                    return;
                }
                self.last_transcript_tap_time = Some(now);
                self.last_transcript_tap_x = mx;
                self.last_transcript_tap_y = my;
                self.transcript_tap_scrolled = false;
                self.lang_selector.show_menu = false;
                self.transcript_pointer_down = true;
                self.transcript_view.on_mouse_down(mx, my);
            }
        }
    }

    fn on_mouse_up(&mut self, state: &mut EngineState) {
        state.keyboard.onscreen.on_mouse_up();
        self.transcript_view.on_trackpad_pointer_up();
        if self.mic_pointer_down {
            let hold_elapsed = self
                .mic_pointer_down_at
                .map(|t| t.elapsed())
                .unwrap_or(Duration::ZERO);
            let over_mic = self.ui_bounds.mic.is_some_and(|r| {
                state.mouse.x >= r.0
                    && state.mouse.x <= r.2
                    && state.mouse.y >= r.1
                    && state.mouse.y <= r.3
            });
            if self.ptt_hold_active {
                self.ptt_hold_active = false;
                self.ptt_hold_started_at = None;
                if !self.ptt_hold_pcm.is_empty() {
                    #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
                    let hold_lang = Some(self.lang_selector.current_language_code());
                    #[cfg(not(all(feature = "whisper", not(target_arch = "wasm32"))))]
                    let hold_lang: Option<&str> = None;
                    match transcribe_waveform_once_with_language(
                        None,
                        &self.ptt_hold_pcm,
                        self.ptt_hold_sample_rate,
                        transcribe_backend_from_env(),
                        hold_lang,
                    ) {
                        Ok(text) => {
                            let t = Self::normalize_text(&text);
                            if !t.is_empty() {
                                self.push_dedup_committed_line(
                                    &t,
                                    Some(HOLD_FINAL_TEXT_COLOR),
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!("transcribe: hold full-clip decode failed: {e}");
                        }
                    }
                }
                self.ptt_hold_pcm.clear();
                self.ptt_hold_last_ingested = None;
            } else if hold_elapsed < HOLD_TO_RECORD_THRESHOLD && over_mic {
                self.live_transcribe_enabled = !self.live_transcribe_enabled;
            }
            self.mic_pointer_down = false;
            self.mic_pointer_down_at = None;
        }
        #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
        {
            if self
                .lang_selector
                .on_button_pointer_up(state.mouse.x, state.mouse.y)
            {
                self.lang_selector.on_pointer_tap_opens_if_closed();
            }
        }
        self.slider_dragging = false;
        self.intensity_slider_dragging = false;
        if self.transcript_pointer_down {
            self.transcript_view.on_mouse_up();
            self.transcript_pointer_down = false;
        }
        self.transcript_touch_started_on_keyboard = false;
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        // Only ignore moves while the contact point is still over the OSK (finger may continue into the transcript).
        if self.transcript_touch_started_on_keyboard {
            let shape = state.frame.shape();
            let height = shape[0] as f32;
            let (_, ky, _, _) = state.keyboard.onscreen.top_edge_coordinates();
            let keyboard_top_px = ky * height;
            if state.mouse.y >= keyboard_top_px - 1.0 {
                return;
            }
        }
        if self.ui_bounds.transcript.is_some() && state.keyboard.onscreen.is_trackpad_mode() {
            if let Some(r) = self.ui_bounds.transcript {
                self.transcript_view.on_trackpad_pointer_move(
                    state.mouse.x,
                    state.mouse.y,
                    r,
                    state.mouse.is_left_clicking,
                );
                return;
            }
        }
        if self.intensity_slider_dragging {
            self.update_waveform_intensity_from_mouse(state);
            return;
        }
        if self.slider_dragging {
            self.update_threshold_from_mouse(state);
            return;
        }
        if self.transcript_pointer_down && state.mouse.is_left_clicking {
            self.transcript_view.on_mouse_move_drag(
                state.mouse.x,
                state.mouse.y,
                state.keyboard.onscreen.is_shown(),
            );
        }
    }

    fn on_scroll(&mut self, state: &mut EngineState, _delta_x: f32, delta_y: f32) {
        let Some((x0, y0, x1, y1)) = self.ui_bounds.transcript else {
            return;
        };
        let shape = state.frame.shape();
        let fh = shape[0] as f32;
        let mx = state.mouse.x;
        let my = state.mouse.y;
        let strictly_in_transcript_box =
            mx >= x0 && mx <= x1 && my >= y0 && my <= y1;
        // With OSK open, wheel/touch may report the pointer over waveform chrome or lateral margins;
        // treat the whole safe strip above the keyboard as scrollable transcript.
        let (_, kty_norm, _, _) = state.keyboard.onscreen.top_edge_coordinates();
        let keyboard_top_px = kty_norm * fh;
        let (_sl, st, _ssw, _ssh) = safe_layout_pixels(state);
        let osk_open = state.keyboard.onscreen.is_shown();
        let fw = shape[1] as f32;
        let relaxed_content_strip = osk_open
            && my >= st
            && my <= keyboard_top_px - OSK_CONTENT_GAP_PX
            && mx >= 0.0
            && mx <= fw;

        if !strictly_in_transcript_box && !relaxed_content_strip {
            return;
        }
        if !delta_y.is_finite() || !(delta_y != 0.0) {
            return;
        }
        self.transcript_tap_scrolled = true;
        self.transcript_view.on_scroll(delta_y);
    }
}
