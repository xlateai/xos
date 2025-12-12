use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyTuple};

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
use super::native_webcam;

/// Python bindings for `xos::video::webcam`
#[pymodule]
pub fn webcam(py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(init_camera_py, m)?)?;
    m.add_function(wrap_pyfunction!(get_resolution_py, m)?)?;
    m.add_function(wrap_pyfunction!(get_frame_py, m)?)?;
    Ok(())
}

#[pyfunction(name="init_camera")]
fn init_camera_py() {
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    native_webcam::init_camera();
}

#[pyfunction(name="get_resolution")]
fn get_resolution_py(py: Python) -> PyObject {
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    {
        let (w, h) = native_webcam::get_resolution();
        PyTuple::new(py, &[w, h]).into()
    }

    #[cfg(any(target_arch = "wasm32", target_os = "ios"))]
    PyTuple::new(py, &[0u32, 0u32]).into()
}

#[pyfunction(name="get_frame")]
fn get_frame_py(py: Python) -> PyObject {
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    {
        let buf = native_webcam::get_frame();
        PyBytes::new(py, &buf).into()
    }

    #[cfg(any(target_arch = "wasm32", target_os = "ios"))]
    PyBytes::new(py, &[]).into()
}
