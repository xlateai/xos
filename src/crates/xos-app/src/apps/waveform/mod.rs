mod canvas;

pub use canvas::WaveformCanvas;

use xos_core::engine::audio;
use xos_core::engine::{Application, EngineState};
#[cfg(not(target_os = "ios"))]
use dialoguer::Select;

pub struct Waveform {
    canvas: WaveformCanvas,
    listener: Option<audio::AudioListener>,
}

impl Waveform {
    pub fn new() -> Self {
        Self {
            canvas: WaveformCanvas::new(),
            listener: None,
        }
    }
}

impl Application for Waveform {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        let all_devices = audio::devices();
        let input_devices: Vec<_> = all_devices.into_iter().filter(|d| d.is_input).collect();

        if input_devices.is_empty() {
            return Err("⚠️ No audio input devices found. On Windows, built-in entries named “… (system audio)” capture the mix for each output. On macOS/Linux, use a mic or a virtual loopback device (e.g. BlackHole).".to_string());
        }

        #[cfg(target_os = "ios")]
        {
            let device = input_devices.first().ok_or("No input devices available")?;
            xos_core::print(&format!("🔊 Attempting to use device: {}", device.name));
            let buffer_duration = 1.0;
            match audio::AudioListener::new(device, buffer_duration) {
                Ok(listener) => {
                    listener.record().map_err(|e| {
                        format!("Failed to start recording: {e}. Make sure microphone permission is granted in Settings.")
                    })?;
                    xos_core::print("✅ Audio listener started successfully");
                    self.listener = Some(listener);
                    Ok(())
                }
                Err(e) => Err(format!(
                    "Failed to initialize audio listener: {e}. On iOS, this usually means microphone permission was denied. Please grant microphone access in Settings > Privacy & Security > Microphone."
                )),
            }
        }

        #[cfg(not(target_os = "ios"))]
        {
            let device_names: Vec<String> = input_devices
                .iter()
                .map(|d| d.input_menu_label())
                .collect();
            let selection = Select::new()
                .with_prompt("Select input device (mic or loopback for system audio)")
                .items(&device_names)
                .default(0)
                .interact()
                .map_err(|e| format!("Failed to get user selection: {e}"))?;
            let device = input_devices
                .get(selection)
                .ok_or("Selected device not found")?;
            xos_core::print(&format!("🔊 Selected device: {}", device.name));
            let buffer_duration = 1.0;
            let listener = audio::AudioListener::new(device, buffer_duration)?;
            listener.record()?;
            self.listener = Some(listener);
            Ok(())
        }
    }

    fn tick(&mut self, state: &mut EngineState) {
        let Some(listener) = &self.listener else {
            return;
        };
        self.canvas.tick_draw(state, listener);
    }

    fn on_mouse_down(&mut self, _state: &mut EngineState) {}
    fn on_mouse_up(&mut self, _state: &mut EngineState) {}
    fn on_mouse_move(&mut self, _state: &mut EngineState) {}
}
