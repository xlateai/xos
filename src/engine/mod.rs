pub mod engine;

#[cfg(not(target_arch = "wasm32"))]
pub mod native_engine;

#[cfg(target_arch = "wasm32")]
pub mod wasm_engine;

#[cfg(feature = "python")]
pub mod py_engine;

#[cfg(not(target_arch = "wasm32"))]
pub use native_engine::start_native;

#[cfg(target_arch = "wasm32")]
pub use wasm_engine::run_web;

#[cfg(feature = "python")]
pub use py_engine::PyApplicationWrapper;

pub use engine::{Application, EngineState, FrameState, MouseState};
