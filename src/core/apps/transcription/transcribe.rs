use crate::apps::waveform::WaveformCanvas;
use crate::engine::audio::{self, transcription::TranscriptionEngine};
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
}

impl TranscribeApp {
    pub fn new() -> Self {
        Self {
            listener: None,
            engine: TranscriptionEngine::new(),
            wave: WaveformCanvas::new(),
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
}

impl Application for TranscribeApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        println!("transcribe: waveform in window · transcript on stdout · Esc to quit");
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
        if self.listener.is_none() {
            fill(&mut state.frame, (BG.0, BG.1, BG.2, BG.3));
            return;
        }

        let (channels, sr) = {
            let l = self.listener.as_ref().expect("checked above");
            (l.get_samples_by_channel(), l.buffer().sample_rate())
        };

        self.engine.process_snapshot(sr, &channels);

        let caption = self.engine.caption().to_string();
        self.log_caption_to_stdout(&caption);

        let l = self.listener.as_ref().expect("checked above");
        self.wave.tick_draw(state, l);
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
