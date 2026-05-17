//! Optional hook for Python `xos.mouse.apply_remote_input` (registered by `xos-app` remote).

use serde_json::Value;
use std::sync::{Mutex, OnceLock};

type Handler = fn(&Value);

static HANDLER: OnceLock<Mutex<Option<Handler>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<Handler>> {
    HANDLER.get_or_init(|| Mutex::new(None))
}

pub fn register_remote_input_handler(handler: Handler) {
    *slot().lock().unwrap() = Some(handler);
}

pub fn apply_remote_input_python(payload: &Value) {
    if let Some(h) = *slot().lock().unwrap() {
        h(payload);
    }
}
