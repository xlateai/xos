use pyo3::prelude::*;
use pyo3::types::PyType;

use crate::engine::Application;

pub struct PyApplicationWrapper {
    py_app: PyObject,
}

impl PyApplicationWrapper {
    pub fn new(py_app: PyObject) -> Self {
        Self { py_app }
    }
}

impl Application for PyApplicationWrapper {
    fn setup(&mut self, width: u32, height: u32) -> Result<(), String> {
        Python::with_gil(|py| {
            let app = self.py_app.as_ref(py);
            app.call_method("setup", (width, height), None)
                .map(|_| ())
                .map_err(|e| format!("setup error: {:?}", e))
        })
    }

    fn tick(&mut self, width: u32, height: u32) -> Vec<u8> {
        Python::with_gil(|py| {
            let app = self.py_app.as_ref(py);
            match app.call_method("tick", (width, height), None) {
                Ok(result) => result.extract::<Vec<u8>>().unwrap_or_default(),
                Err(_) => vec![],
            }
        })
    }
}

// Optional ergonomic base class
#[pyclass(subclass)]
pub struct ApplicationBase;

#[pymethods]
impl ApplicationBase {
    #[new]
    fn new() -> Self {
        ApplicationBase
    }

    fn setup(&mut self, _width: u32, _height: u32) {}
    fn tick(&mut self, _width: u32, _height: u32) -> Vec<u8> {
        vec![]
    }
}
