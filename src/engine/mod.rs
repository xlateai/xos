pub mod engine;

#[cfg(not(target_arch = "wasm32"))]
pub mod native_engine;

#[cfg(target_arch = "wasm32")]
pub mod wasm_engine;

#[cfg(target_os = "ios")]
pub mod ios_ffi;

// py_engine is now defined in lib.rs as an inline module

#[cfg(not(target_arch = "wasm32"))]
pub use native_engine::start_native;

#[cfg(target_arch = "wasm32")]
pub use wasm_engine::run_web;

#[cfg(feature = "python")]
pub use crate::py_engine::PyApplicationWrapper;

pub use engine::{Application, EngineState, MouseState, FrameState, SafeRegionBoundingRectangle};
