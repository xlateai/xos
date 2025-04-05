#[cfg(target_arch = "wasm32")]
pub mod wasm_webcam;
#[cfg(not(target_arch = "wasm32"))]
pub mod native_webcam;

#[cfg(target_arch = "wasm32")]
pub use wasm_webcam::*;
#[cfg(not(target_arch = "wasm32"))]
pub use native_webcam::*;

#[cfg(feature = "python")]
#[cfg(not(target_arch = "wasm32"))]
pub mod py_webcam;