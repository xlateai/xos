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
    fn setup(&mut self, state: &mut EngineState) -> Result<(), String> {
        Python::with_gil(|py| {
            let app = self.py_app.bind(py);
            let state_obj = PyEngineState::new(py, state);
            app.call_method1("setup", (state_obj,))
                .map(|_| ())
                .map_err(|e| format!("setup error: {:?}", e))
        })
    }

    fn tick(&mut self, state: &mut EngineState) {
        Python::with_gil(|py| {
            let app = self.py_app.bind(py);
            let state_obj = PyEngineState::new(py, state);
            let _ = app.call_method1("tick", (state_obj,));
            // Force garbage collection to ensure no stale references
            let _ = py.run_bound("import gc; gc.collect()", None, None);
        });
    }

    fn on_mouse_down(&mut self, _state: &mut EngineState) {
        // Empty implementation
    }

    fn on_mouse_up(&mut self, _state: &mut EngineState) {
        // Empty implementation
    }

    fn on_mouse_move(&mut self, _state: &mut EngineState) {
        // Empty implementation
    }
}

// Add the unsendable marker to allow non-thread-safe raw pointers
#[pyclass(unsendable)]
pub struct PyFrameState {
    #[pyo3(get)]
    pub width: u32,
    #[pyo3(get)]
    pub height: u32,
    // Store a reference to the buffer directly
    buffer_ptr: *mut [u8],
    buffer_len: usize,
}

// Make PyFrameState safe to use from Python
unsafe impl pyo3::AsPyPointer for PyFrameState {
    fn as_ptr(&self) -> *mut pyo3::ffi::PyObject {
        self as *const _ as *mut _
    }
}

#[pymethods]
impl PyFrameState {
    #[getter]
    fn buffer<'py>(&self, py: Python<'py>) -> PyResult<Py<PyByteArray>> {
        unsafe {
            // Create a slice from our raw pointer
            let slice = std::slice::from_raw_parts_mut(
                self.buffer_ptr as *mut u8,
                self.buffer_len
            );
            
            // Create a PyByteArray directly referencing our buffer
            // This achieves true zero-copy
            let byte_array = PyByteArray::new_bound(py, slice);
            Ok(byte_array.unbind())
        }
    }
}

#[pyclass]
pub struct PyMouseState {
    #[pyo3(get)]
    pub x: f32,
    #[pyo3(get)]
    pub y: f32,
    #[pyo3(get)]
    pub is_down: bool,
}

// Also mark this as unsendable since it contains the frame state
#[pyclass(unsendable)]
pub struct PyEngineState {
    #[pyo3(get)]
    pub frame: Py<PyFrameState>,
    #[pyo3(get)]
    pub mouse: Py<PyMouseState>,
}

impl PyEngineState {
    // Create a new PyEngineState that has direct access to the original state
    fn new(py: Python, state: &mut EngineState) -> Py<Self> {
        // Get a pointer to the buffer
        let buffer_ptr = state.frame.buffer.as_mut_slice() as *mut [u8];
        let buffer_len = state.frame.buffer.len();
        
        // Create the frame state with direct pointer to the buffer
        let frame_state = PyFrameState {
            width: state.frame.width,
            height: state.frame.height,
            buffer_ptr,
            buffer_len,
        };
        
        // Create the mouse state
        let mouse_state = PyMouseState {
            x: state.mouse.x,
            y: state.mouse.y,
            is_down: state.mouse.is_down,
        };
        
        // Create Python objects for each component
        let frame = Py::new(py, frame_state).unwrap();
        let mouse = Py::new(py, mouse_state).unwrap();
        
        // Create the engine state
        Py::new(py, Self { 
            frame, 
            mouse,
        }).unwrap()
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

    fn setup(&mut self, _state: &PyEngineState) -> PyResult<()> {
        Ok(())
    }
    
    fn tick(&mut self, _state: &PyEngineState) -> PyResult<()> {
        Ok(())
    }
}