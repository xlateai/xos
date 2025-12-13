#[cfg(target_arch = "wasm32")]
pub mod wasm_webcam;
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
pub mod native_webcam;

#[cfg(target_arch = "wasm32")]
pub use wasm_webcam::*;
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
pub use native_webcam::*;

// iOS stubs
#[cfg(target_os = "ios")]
pub fn init_camera() {
    // Camera not available on iOS in this implementation
}

#[cfg(target_os = "ios")]
pub fn get_resolution() -> (u32, u32) {
    (0, 0)
}

#[cfg(target_os = "ios")]
pub fn get_frame() -> Vec<u8> {
    vec![]
}

#[cfg(feature = "python")]
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
pub mod py_webcam;