use once_cell::sync::OnceCell;
use wasm_bindgen_futures::spawn_local;
use web_sys::console;

use super::wasm_listener::init_microphone;

#[derive(Clone)]
pub struct AudioDevice {
    pub name: String,
    pub is_input: bool,
    pub is_output: bool,
}

// Global one-time mic initializer
static AUDIO_INIT: OnceCell<()> = OnceCell::new();

/// Returns a fake device that represents the browser mic
pub fn all() -> Vec<AudioDevice> {
    AUDIO_INIT.get_or_init(|| {
        spawn_local(async {
            match init_microphone().await {
                Ok(_) => console::log_1(&"🎤 Mic initialized".into()),
                Err(err) => console::error_1(&format!("❌ Mic init failed: {err:?}").into()),
            }
        });
    });

    vec![AudioDevice {
        name: "Web Mic".to_string(),
        is_input: true,
        is_output: false,
    }]
}

/// Optional diagnostic function
pub fn print_all() {
    console::log_1(&"⚠️ audio::print_devices() in WASM always returns a simulated Web Mic.".into());
}
