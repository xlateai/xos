//! Remote desktop preview: two-node LAN mesh (host = viewer, one peer = streamer).
//! Start the **viewer** first (`xos app remote`), then the **streamer** on the other machine.
//!
//! - **Windows**: GDI capture + `SetCursorPos` / `mouse_event` on the streamer.
//! - **macOS**: screen capture + mouse on the streamer (grant **Screen Recording** for the `xos` binary).
//!
//! Implementation: [`remote::RemoteApp`]. Python-facing sketch: `remote.py`.

mod remote;

pub use remote::RemoteApp;
