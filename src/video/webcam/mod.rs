#[cfg(target_arch = "wasm32")]
pub mod wasm_webcam;
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
pub mod native_webcam;

#[cfg(target_arch = "wasm32")]
pub use wasm_webcam::*;
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
pub use native_webcam::*;

#[cfg(feature = "python")]
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
pub mod py_webcam;