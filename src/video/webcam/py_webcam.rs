// NOTE: This file previously used pyo3 for Python bindings.
// It needs to be reimplemented using rustpython-vm bindings.
// For now, this module is disabled.

// TODO: Reimplement webcam Python bindings using rustpython-vm
// The rustpython API is different from pyo3 - we'll need to:
// 1. Create rustpython function bindings
// 2. Expose webcam functions to rustpython VM
// 3. Implement Python functions using rustpython's API

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
use super::native_webcam;

// Placeholder functions - will be reimplemented with rustpython
pub fn init_camera_py() {
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    native_webcam::init_camera();
}

pub fn get_resolution_py() -> Result<(u32, u32), String> {
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    {
        Ok(native_webcam::get_resolution())
    }
    #[cfg(any(target_arch = "wasm32", target_os = "ios"))]
    {
        Ok((0, 0))
    }
}

pub fn get_frame_py() -> Result<Vec<u8>, String> {
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    {
        Ok(native_webcam::get_frame())
    }
    #[cfg(any(target_arch = "wasm32", target_os = "ios"))]
    {
        Ok(vec![])
    }
}
