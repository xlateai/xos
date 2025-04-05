use pyo3::prelude::*;
use pyo3::types::PyByteArray;

use crate::engine::{Application, EngineState};

pub struct PyApplicationWrapper {
    py_app: PyObject,
}

impl PyApplicationWrapper {
    pub fn new(py_app: PyObject) -> Self {
        Self { py_app }
    }
}

impl Application for PyApplicationWrapper {
    fn setup(&mut self, state: &EngineState) -> Result<(), String> {
        Python::with_gil(|py| {
            let app = self.py_app.bind(py);
            let state_obj = Py::new(py, PyEngineState::from(state)).unwrap();
            app.call_method("setup", (state_obj,), None)
                .map(|_| ())
                .map_err(|e| format!("setup error: {:?}", e))
        })
    }

    fn tick(&mut self, state: &EngineState) {
        Python::with_gil(|py| {
            let app = self.py_app.bind(py);
            let state_obj = Py::new(py, PyEngineState::from(state)).unwrap();
            let _ = app.call_method("tick", (state_obj,), None);
        });
    }
}

#[pyclass]
#[derive(Clone)]
pub struct PyFrameState {
    #[pyo3(get)]
    pub width: u32,
    #[pyo3(get)]
    pub height: u32,
    buffer: Vec<u8>,
}

#[pymethods]
impl PyFrameState {
    #[getter]
    fn buffer<'py>(&self, py: Python<'py>) -> &'py PyAny {
        PyByteArray::new(py, &self.buffer).into()
    }
}

#[pyclass]
#[derive(Clone)]
pub struct PyMouseState {
    #[pyo3(get)]
    pub x: f32,
    #[pyo3(get)]
    pub y: f32,
    #[pyo3(get)]
    pub is_down: bool,
}

#[pyclass]
#[derive(Clone)]
pub struct PyEngineState {
    #[pyo3(get)]
    pub frame: PyFrameState,
    #[pyo3(get)]
    pub mouse: PyMouseState,
}


impl From<&EngineState> for PyEngineState {
    fn from(state: &EngineState) -> Self {
        PyEngineState {
            frame: PyFrameState {
                width: state.frame.width,
                height: state.frame.height,
                buffer: state.frame.buffer.borrow().clone(),
            },
            mouse: PyMouseState {
                x: state.mouse.x,
                y: state.mouse.y,
                is_down: state.mouse.is_down,
            },
        }
    }
}

#[pyclass(subclass)]
pub struct ApplicationBase;

#[pymethods]
impl ApplicationBase {
    #[new]
    fn new() -> Self {
        ApplicationBase
    }

    fn setup(&mut self, _state: &PyEngineState) {}
    fn tick(&mut self, _state: &PyEngineState) {}
}
