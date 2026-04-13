use crate::engine::audio::{self, transcription::TranscriptionEngine};
use crate::engine::{Application, EngineState};
#[cfg(not(target_os = "ios"))]
use dialoguer::Select;

pub struct TranscribeApp {
    listener: Option<audio::AudioListener>,
    engine: TranscriptionEngine,
    /// Last caption printed (excludes device line — device is printed once in setup).
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
}

impl Application for TranscribeApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        crate::print("transcribe: terminal mode (Ctrl+C to stop)\n");

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
            crate::print(&format!(
                "transcribe: {} @ {} Hz\n",
                device.name,
                listener.buffer().sample_rate()
            ));
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
            crate::print(&format!(
                "transcribe: {} @ {} Hz\n",
                device.name,
                listener.buffer().sample_rate()
            ));
            self.engine
                .set_device_hint(device.name.as_str(), listener.buffer().sample_rate());
            self.listener = Some(listener);
            Ok(())
        }
    }

    fn tick(&mut self, state: &mut EngineState) {
        if let Some(listener) = &self.listener {
            let channels = listener.get_samples_by_channel();
            let sr = listener.buffer().sample_rate();
            self.engine.process_snapshot(sr, &channels);
        }

        let caption = if self.listener.is_some() {
            self.engine.caption().to_string()
        } else {
            "No audio listener.".to_string()
        };

        if caption != self.last_caption_out {
            self.last_caption_out = caption.clone();
            crate::print(&format!("{caption}\n"));
        }

        // Keep frame delta / F3 state coherent for the headless host (no GPU work).
        let _ = state;
    }

    fn on_mouse_down(&mut self, _state: &mut EngineState) {}

    fn on_mouse_up(&mut self, _state: &mut EngineState) {}

    fn on_mouse_move(&mut self, _state: &mut EngineState) {}
}
