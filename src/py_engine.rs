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
    
            if let Ok(obj) = app.call_method1("tick", (state_obj,)) {
                if let Ok(array) = obj.getattr("tobytes") {
                    if let Ok(bytes_obj) = array.call0() {
                        if let Ok(pybytes) = bytes_obj.downcast::<pyo3::types::PyBytes>() {
                            let data = pybytes.as_bytes();
                            let dst = &mut state.frame.buffer;
                            let len = dst.len().min(data.len());
                            dst[..len].copy_from_slice(&data[..len]);
                        }
                    }
                }
            }
        });
    }
    

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        Python::with_gil(|py| {
            let app = self.py_app.bind(py);
            let state_obj = PyEngineState::new(py, state);
            let _ = app.call_method1("on_mouse_down", (state_obj,));
        });
    }

    fn on_mouse_up(&mut self, state: &mut EngineState) {
        Python::with_gil(|py| {
            let app = self.py_app.bind(py);
            let state_obj = PyEngineState::new(py, state);
            let _ = app.call_method1("on_mouse_up", (state_obj,));
        });
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        Python::with_gil(|py| {
            let app = self.py_app.bind(py);
            let state_obj = PyEngineState::new(py, state);
            let _ = app.call_method1("on_mouse_move", (state_obj,));
        });
    }
}

#[pyclass(unsendable)]
pub struct PyFrameState {
    #[pyo3(get)]
    pub width: u32,
    #[pyo3(get)]
    pub height: u32,
    // Store a direct pointer to the Vec<u8>
    rust_buffer: *mut Vec<u8>,
}

#[pymethods]
impl PyFrameState {
    #[getter]
    fn buffer<'py>(&self, py: Python<'py>) -> PyResult<Py<PyByteArray>> {
        unsafe {
            // Get direct access to the Vec<u8>
            let buffer = &mut *self.rust_buffer;
            let byte_array = PyByteArray::new_bound(py, buffer.as_mut_slice());
            Ok(byte_array.unbind())
        }
    }
    
    // Debug method that can be called from Python
    fn debug_print(&self) -> PyResult<()> {
        unsafe {
            let buffer = &*self.rust_buffer;
            println!("Buffer pointer: {:p}, len: {}", buffer.as_ptr(), buffer.len());
            println!("First few bytes: {:?}", &buffer[..buffer.len().min(16)]);
        }
        Ok(())
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

#[pyclass(unsendable)]
pub struct PyEngineState {
    #[pyo3(get)]
    pub frame: Py<PyFrameState>,
    #[pyo3(get)]
    pub mouse: Py<PyMouseState>,
}

impl PyEngineState {
    fn new(py: Python, state: &mut EngineState) -> Py<Self> {
        // Store a pointer to the actual Vec<u8>
        let buffer_ptr = &mut state.frame.buffer as *mut Vec<u8>;
        
        let frame_state = PyFrameState {
            width: state.frame.width,
            height: state.frame.height,
            rust_buffer: buffer_ptr,
        };
        
        let mouse_state = PyMouseState {
            x: state.mouse.x,
            y: state.mouse.y,
            is_down: state.mouse.is_down,
        };
        
        let frame = Py::new(py, frame_state).unwrap();
        let mouse = Py::new(py, mouse_state).unwrap();
        
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

    fn setup(&self, state: Bound<'_, PyEngineState>) -> PyResult<()> {
        // Get buffer for debugging
        Python::with_gil(|py| {
            if let Ok(frame) = state.getattr("frame") {
                let _ = frame.call_method0("debug_print");
            }
        });
        Ok(())
    }
    
    fn tick(&self, state: Bound<'_, PyEngineState>) -> PyResult<()> {
        // Default implementation just debug prints
        Python::with_gil(|py| {
            if let Ok(frame) = state.getattr("frame") {
                let _ = frame.call_method0("debug_print");
            }
        });
        Ok(())
    }
    
    fn on_mouse_down(&self, _state: Bound<'_, PyEngineState>) -> PyResult<()> {
        Ok(())
    }
    
    fn on_mouse_up(&self, _state: Bound<'_, PyEngineState>) -> PyResult<()> {
        Ok(())
    }
    
    fn on_mouse_move(&self, _state: Bound<'_, PyEngineState>) -> PyResult<()> {
        Ok(())
    }
}
