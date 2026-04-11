//! Remote desktop preview: two-node LAN mesh (host = viewer, one peer = streamer).
//! Start the **viewer** first (`xos app remote`), then the **streamer** on the other machine.
//!
//! - **Windows**: GDI capture + `SetCursorPos` / `mouse_event` on the streamer.
//! - **macOS**: screen capture + mouse on the streamer (grant **Screen Recording** for the `xos` binary).
//!
//! Implementation: [`remote::RemoteApp`]. Python-facing sketch: `remote.py`.

mod remote;

pub use remote::RemoteApp;

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos")
))]
pub use remote::capture_scaled_jpeg;

use serde_json::Value;
use std::cell::RefCell;

thread_local! {
    static PYTHON_REMOTE_INPUT_PREV: RefCell<(bool, bool)> = RefCell::new((false, false));
}

/// Apply one coalesced remote-input payload from Python (`xos.mouse.apply_remote_input`).
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos")
))]
pub fn apply_remote_input_python(payload: &Value) {
    PYTHON_REMOTE_INPUT_PREV.with(|p| {
        let mut g = p.borrow_mut();
        remote::apply_remote_input(payload, &mut g.0, &mut g.1);
    });
}

#[cfg(not(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos")
)))]
pub fn apply_remote_input_python(_payload: &Value) {}
