use crate::apps::waveform::WaveformCanvas;
use crate::ai::transcription::TranscriptionEngine;
use crate::engine::audio;
use crate::engine::{Application, EngineState};
use crate::rasterizer::fill;
#[cfg(not(target_os = "ios"))]
use dialoguer::Select;
use std::io::{self, Write};

const BG: (u8, u8, u8, u8) = (12, 14, 20, 255);

pub struct TranscribeApp {
    listener: Option<audio::AudioListener>,
    engine: TranscriptionEngine,
    wave: WaveformCanvas,
    last_caption_out: String,
    /// True after we drew the rolling transcript with `\r` — need a final newline on exit.
    live_stdout_line: bool,
}

impl TranscribeApp {
    pub fn new() -> Self {
        Self {
            listener: None,
            engine: TranscriptionEngine::new(),
            wave: WaveformCanvas::new(),
            last_caption_out: String::new(),
            live_stdout_line: false,
        }
    }

    fn pause_input(&self) {
        if let Some(l) = &self.listener {
            let _ = l.pause();
        }
    }

    fn log_transcript_line_to_stdout(&mut self, line: &str) {
        if line.is_empty() {
            if self.live_stdout_line {
                print!("\r\x1b[2K");
                let _ = io::stdout().flush();
                self.live_stdout_line = false;
            }
            self.last_caption_out.clear();
            return;
        }
        if line == self.last_caption_out {
            return;
        }
        self.last_caption_out = line.to_string();
        print!("\r\x1b[2K{}", line);
        let _ = io::stdout().flush();
        self.live_stdout_line = true;
    }
}

impl Drop for TranscribeApp {
    fn drop(&mut self) {
        self.engine.flush_live_to_stdout_commits();
        for line in self.engine.drain_stdout_commits() {
            if self.live_stdout_line {
                print!("\r\x1b[2K");
                self.live_stdout_line = false;
            }
            println!("{}", line);
        }
        self.pause_input();
        if self.live_stdout_line {
            println!();
        }
    }
}

impl Application for TranscribeApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        println!("transcribe: waveform in window · transcript on stdout (scrollback + live line) · Esc to quit");
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
        if self.listener.is_none() {
            fill(&mut state.frame, (BG.0, BG.1, BG.2, BG.3));
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

        let l = self.listener.as_ref().expect("checked above");
        self.wave.tick_draw(state, l);

        self.engine.process_snapshot(sr, &channels, ingested);

        for line in self.engine.drain_stdout_commits() {
            if self.live_stdout_line {
                print!("\r\x1b[2K");
                self.live_stdout_line = false;
            }
            println!("{}", line);
        }

        let line = self.engine.caption().to_string();
        self.log_transcript_line_to_stdout(&line);
    }

    fn on_key_char(&mut self, _state: &mut EngineState, ch: char) {
        if ch == '\u{1b}' {
            self.pause_input();
            crate::engine::native_engine::request_exit();
        }
    }

    fn prepare_shutdown(&mut self, _state: &mut EngineState) {
        self.engine.flush_live_to_stdout_commits();
        for line in self.engine.drain_stdout_commits() {
            if self.live_stdout_line {
                print!("\r\x1b[2K");
                self.live_stdout_line = false;
            }
            println!("{}", line);
        }
        if let Some(listener) = self.listener.take() {
            let _ = listener.pause();
        }
    }

    fn on_mouse_down(&mut self, _state: &mut EngineState) {}

    fn on_mouse_up(&mut self, _state: &mut EngineState) {}

    fn on_mouse_move(&mut self, _state: &mut EngineState) {}
}
