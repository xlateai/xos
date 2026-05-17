//! Optional remote desktop JPEG capture for Python mesh (`xos-app` registers).

use std::sync::{Mutex, OnceLock};

type CaptureFn = fn() -> Option<(Vec<u8>, u32, u32)>;

static HOOK: OnceLock<Mutex<Option<CaptureFn>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<CaptureFn>> {
    HOOK.get_or_init(|| Mutex::new(None))
}

pub fn register_remote_jpeg_capture(f: CaptureFn) {
    *slot().lock().unwrap() = Some(f);
}

pub fn capture_scaled_jpeg() -> Option<(Vec<u8>, u32, u32)> {
    slot().lock().unwrap().and_then(|f| f())
}
