use crate::engine::audio::{self, transcription::TranscriptionEngine};
use crate::engine::{Application, EngineState};
use crate::rasterizer::fill;
use crate::ui::UiText;
#[cfg(not(target_os = "ios"))]
use dialoguer::Select;

const BG: (u8, u8, u8, u8) = (12, 14, 20, 255);

pub struct TranscribeApp {
    listener: Option<audio::AudioListener>,
    engine: TranscriptionEngine,
}

impl TranscribeApp {
    pub fn new() -> Self {
        Self {
            listener: None,
            engine: TranscriptionEngine::new(),
        }
    }
}

impl Application for TranscribeApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        let all_devices = audio::devices();
        let input_devices: Vec<_> = all_devices.into_iter().filter(|d| d.is_input).collect();

        if input_devices.is_empty() {
            return Err("No audio input devices found. On macOS, pick a mic or a loopback driver (e.g. BlackHole) so system audio appears as an input.".to_string());
        }

        #[cfg(target_os = "ios")]
        {
            let device = input_devices.first().ok_or("No input devices available")?;
            crate::print(&format!("Transcribe: using {}", device.name));
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

            crate::print(&format!("Transcribe: using {}", device.name));

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

        if let Some(listener) = &self.listener {
            let channels = listener.get_samples_by_channel();
            let sr = listener.buffer().sample_rate();
            self.engine.process_snapshot(sr, &channels);
        }

        let text = if self.listener.is_some() {
            self.engine.full_display()
        } else {
            "No audio listener.".to_string()
        };

        let buf = state.frame_buffer_mut();
        let ui = UiText {
            text,
            x1_norm: 0.05,
            y1_norm: 0.05,
            x2_norm: 0.95,
            y2_norm: 0.95,
            color: (235, 238, 245, 255),
            hitboxes: false,
            baselines: false,
            font_size_px: font_px.max(10.0),
        };

        if let Err(e) = ui.render(buf, width, height) {
            crate::print(&format!("Transcribe: UiText render error: {e}"));
        }
    }

    fn on_mouse_down(&mut self, _state: &mut EngineState) {}

    fn on_mouse_up(&mut self, _state: &mut EngineState) {}

    fn on_mouse_move(&mut self, _state: &mut EngineState) {}
}
