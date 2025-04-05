use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyTuple};

#[cfg(not(target_arch = "wasm32"))]
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
    #[cfg(not(target_arch = "wasm32"))]
    native_webcam::init_camera();
}

#[pyfunction(name="get_resolution")]
fn get_resolution_py(py: Python) -> PyObject {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let (w, h) = native_webcam::get_resolution();
        PyTuple::new(py, &[w, h]).into()
    }

    #[cfg(target_arch = "wasm32")]
    PyTuple::new(py, &[0u32, 0u32]).into()
}

#[pyfunction(name="get_frame")]
fn get_frame_py(py: Python) -> PyObject {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let buf = native_webcam::get_frame();
        PyBytes::new(py, &buf).into()
    }

    #[cfg(target_arch = "wasm32")]
    PyBytes::new(py, &[]).into()
}
