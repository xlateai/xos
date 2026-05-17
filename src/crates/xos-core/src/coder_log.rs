use std::sync::{Mutex, OnceLock};

type Handler = fn(&str);

static HANDLER: OnceLock<Mutex<Option<Handler>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<Handler>> {
    HANDLER.get_or_init(|| Mutex::new(None))
}

pub fn register_coder_log_handler(handler: Handler) {
    *slot().lock().unwrap() = Some(handler);
}

pub fn log_to_coder(message: &str) {
    if let Some(h) = *slot().lock().unwrap() {
        h(message);
    }
}
