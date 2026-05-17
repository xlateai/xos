mod game_runner;
mod start;

pub mod apps;
pub mod python_text;
pub mod python_ui;

#[cfg(target_os = "ios")]
pub mod ios_ffi;

pub use game_runner::run_game;
pub use start::{start, start_wasm};
pub use xos_core::launch_ios_app;

static HOOKS: std::sync::Once = std::sync::Once::new();

pub fn init_hooks() {
    HOOKS.call_once(|| {
        xos_core::ui_module::register_make_ui_module(python_ui::make_ui_module);
        xos_core::coder_log::register_coder_log_handler(apps::coder::logging::log_to_coder);
        #[cfg(all(
            not(target_arch = "wasm32"),
            not(target_os = "ios"),
            any(target_os = "windows", target_os = "macos")
        ))]
        xos_core::remote_input::register_remote_input_handler(
            apps::remote::apply_remote_input_python,
        );
        #[cfg(all(
            not(target_arch = "wasm32"),
            not(target_os = "ios"),
            any(target_os = "windows", target_os = "macos")
        ))]
        xos_core::remote_capture::register_remote_jpeg_capture(apps::remote::capture_scaled_jpeg);
        #[cfg(all(
            not(target_arch = "wasm32"),
            any(target_os = "macos", target_os = "windows")
        ))]
        {
            use xos_core::monitors::MonitorDescriptor;
            fn list() -> Vec<MonitorDescriptor> {
                apps::remote::monitors::system_monitors()
                    .into_iter()
                    .map(|m| MonitorDescriptor {
                        native_width: m.native_width,
                        native_height: m.native_height,
                        origin_x: m.origin_x,
                        origin_y: m.origin_y,
                        refresh_rate_hz: m.refresh_rate_hz,
                        is_primary: m.is_primary,
                        name: m.name,
                        native_id: m.native_id,
                        stream_width: m.stream_width,
                        stream_height: m.stream_height,
                    })
                    .collect()
            }
            fn capture(idx: usize) -> Option<(Vec<u8>, u32, u32)> {
                apps::remote::monitors::system_monitor_capture_scaled_rgba(idx)
            }
            fn snapshot(idx: usize) -> Option<(Vec<u8>, u32, u32)> {
                apps::remote::monitor_stream::snapshot(idx)
            }
            xos_core::monitors::register_monitor_hooks(list, capture, snapshot);
        }
    });
}
