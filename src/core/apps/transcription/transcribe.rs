use crate::engine::audio::{self, transcription::TranscriptionEngine};
use crate::engine::{Application, EngineState};
use crate::rasterizer::fill;
use crate::rasterizer::fill_rect_buffer;
use crate::ui::UiText;
#[cfg(not(target_os = "ios"))]
use dialoguer::Select;
use std::io::{self, Write};

const BG: (u8, u8, u8, u8) = (12, 14, 20, 255);
/// Bars across the bottom for mic level (not Whisper decode).
const WAVEFORM_COLS: usize = 160;

pub struct TranscribeApp {
    listener: Option<audio::AudioListener>,
    engine: TranscriptionEngine,
    last_caption_out: String,
}

impl TranscribeApp {
    pub fn new() -> Self {
        Self {
            listener: None,
            engine: TranscriptionEngine::new(),
            last_caption_out: String::new(),
        }
    }

    fn log_caption_to_stdout(&mut self, caption: &str) {
        if caption == self.last_caption_out {
            return;
        }
        self.last_caption_out = caption.to_string();
        println!("{caption}");
        let _ = io::stdout().flush();
    }

    /// Recent mono samples → bottom strip of vertical bars (live mic sanity check).
    fn draw_waveform_strip(
        buffer: &mut [u8],
        width: usize,
        height: usize,
        mono: &[f32],
    ) {
        let strip_h = ((height as f32) * 0.12).max(28.0).round() as i32;
        let y0 = height as i32 - strip_h;
        let strip_color = (22, 26, 34, 255);
        for y in y0.max(0)..height as i32 {
            fill_rect_buffer(buffer, width, height, 0, y, width as i32, y + 1, strip_color);
        }
        if mono.is_empty() || width == 0 {
            return;
        }
        let cols = WAVEFORM_COLS.min(width);
        let chunk = (mono.len() / cols).max(1);
        let start = mono.len().saturating_sub(chunk * cols);
        let bar_color = (80, 200, 220, 255);
        let col_w = (width as f32 / cols as f32).max(1.0);
        for c in 0..cols {
            let i0 = start + c * chunk;
            let i1 = (i0 + chunk).min(mono.len());
            let peak = mono[i0..i1]
                .iter()
                .map(|s| s.abs())
                .fold(0.0f32, f32::max);
            let amp = (peak * 6.0).clamp(0.0, 1.0);
            let bar_h = (amp * strip_h as f32).max(2.0).round() as i32;
            let x0 = ((c as f32) * col_w).floor() as i32;
            let x1 = (((c + 1) as f32) * col_w).ceil() as i32;
            let bx1 = (x1).min(width as i32);
            let by0 = height as i32 - bar_h;
            fill_rect_buffer(
                buffer,
                width,
                height,
                x0.max(0),
                by0.max(y0),
                bx1.max(x0 + 1),
                height as i32,
                bar_color,
            );
        }
    }
}

impl Application for TranscribeApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        println!("transcribe: window + live waveform strip · transcript on stdout · Esc to quit");
        let _ = io::stdout().flush();

        let all_devices = audio::devices();
        let input_devices: Vec<_> = all_devices.into_iter().filter(|d| d.is_input).collect();

        if input_devices.is_empty() {
            return Err("No audio input devices found. On macOS, pick a mic or a loopback driver (e.g. BlackHole) so system audio appears as an input.".to_string());
        }

        #[cfg(target_os = "ios")]
        {
            let device = input_devices.first().ok_or("No input devices available")?;
            #[cfg(all(
                feature = "whisper_ct2",
                not(target_os = "ios"),
                not(target_arch = "wasm32")
            ))]
            let buffer_duration = 10.0_f32;
            #[cfg(not(all(
                feature = "whisper_ct2",
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
            let names: Vec<String> = input_devices.iter().map(|d| d.name.clone()).collect();
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
                feature = "whisper_ct2",
                not(target_os = "ios"),
                not(target_arch = "wasm32")
            ))]
            let buffer_duration = 10.0_f32;
            #[cfg(not(all(
                feature = "whisper_ct2",
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
        fill(&mut state.frame, (BG.0, BG.1, BG.2, BG.3));

        let shape = state.frame.shape();
        let height = shape[0] as usize;
        let width = shape[1] as usize;
        if width == 0 || height == 0 {
            return;
        }

        let font_px = 18.0_f32 * state.f3_ui_scale_multiplier();

        let wave_mono = if let Some(listener) = &self.listener {
            let channels = listener.get_samples_by_channel();
            let sr = listener.buffer().sample_rate();
            self.engine.process_snapshot(sr, &channels);
            channels.into_iter().next().unwrap_or_default()
        } else {
            Vec::new()
        };

        let caption = if self.listener.is_some() {
            self.engine.caption().to_string()
        } else {
            "No audio listener.".to_string()
        };

        self.log_caption_to_stdout(&caption);

        let buf = state.frame_buffer_mut();
        Self::draw_waveform_strip(buf, width, height, &wave_mono);

        let display = if self.listener.is_some() {
            format!("{}\n\n{}", self.engine.device_hint(), caption)
        } else {
            caption.clone()
        };
        let ui = UiText {
            text: display,
            x1_norm: 0.04,
            y1_norm: 0.04,
            x2_norm: 0.96,
            y2_norm: 0.82,
            color: (235, 238, 245, 255),
            hitboxes: false,
            baselines: false,
            font_size_px: font_px.max(10.0),
        };
        if let Err(e) = ui.render(buf, width, height) {
            eprintln!("transcribe: UiText render error: {e}");
        }
    }

    fn on_key_char(&mut self, _state: &mut EngineState, ch: char) {
        if ch == '\u{1b}' {
            crate::engine::native_engine::request_exit();
        }
    }

    fn on_mouse_down(&mut self, _state: &mut EngineState) {}

    fn on_mouse_up(&mut self, _state: &mut EngineState) {}

    fn on_mouse_move(&mut self, _state: &mut EngineState) {}
}
