//! Python UI for `xos app study` — [`launcher`] loads `study.py` beside this module.

#[cfg(not(target_arch = "wasm32"))]
pub mod launcher;
