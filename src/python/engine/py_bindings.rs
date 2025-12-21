use rustpython_vm::{PyResult, VirtualMachine, PyObjectRef};
use crate::engine::FrameState;

/// Python wrapper for Array<u8>
#[derive(Debug)]
pub struct PyArray {
    // We'll store a raw pointer since we can't directly store the array
    // The actual array lives in the EngineState
    pub ptr: *mut u8,
    pub shape: Vec<usize>,
    pub device: String,
}

impl PyArray {
    pub fn from_frame_state(frame: &mut FrameState) -> Self {
        let shape = frame.shape();
        let buffer = frame.buffer_mut();
        Self {
            ptr: buffer.as_mut_ptr(),
            shape,
            device: "cpu".to_string(),
        }
    }
    
    pub fn get_buffer_slice(&self) -> &[u8] {
        let len: usize = self.shape.iter().product();
        unsafe { std::slice::from_raw_parts(self.ptr, len) }
    }
    
    pub fn get_buffer_slice_mut(&mut self) -> &mut [u8] {
        let len: usize = self.shape.iter().product();
        unsafe { std::slice::from_raw_parts_mut(self.ptr, len) }
    }
}

/// Python wrapper for FrameState
#[derive(Debug)]
pub struct PyFrameState {
    // Store pointer to the actual FrameState in the engine
    pub frame_ptr: *mut FrameState,
}

impl PyFrameState {
    pub fn new(frame: &mut FrameState) -> Self {
        Self {
            frame_ptr: frame as *mut FrameState,
        }
    }
    
    fn get_frame(&self) -> &FrameState {
        unsafe { &*self.frame_ptr }
    }
    
    fn get_frame_mut(&mut self) -> &mut FrameState {
        unsafe { &mut *self.frame_ptr }
    }
}

// For now, let's create simple Python objects using dicts
// In the future, we can use pyclass to make them proper Python classes

pub fn create_py_array(vm: &VirtualMachine, frame: &mut FrameState) -> PyResult {
    let shape = frame.shape();
    let buffer = frame.buffer_mut();
    
    let dict = vm.ctx.new_dict();
    dict.set_item("shape", vm.ctx.new_tuple(shape.iter().map(|&s| vm.ctx.new_int(s).into()).collect()).into(), vm)?;
    dict.set_item("device", vm.ctx.new_str("cpu").into(), vm)?;
    
    // Create a Python list that directly wraps the buffer
    // This is tricky - we need to create PyInt objects for each byte
    let py_buffer: Vec<PyObjectRef> = buffer.iter().map(|&b| vm.ctx.new_int(b).into()).collect();
    dict.set_item("data", vm.ctx.new_list(py_buffer).into(), vm)?;
    
    // Add metadata
    dict.set_item("dtype", vm.ctx.new_str("uint8").into(), vm)?;
    dict.set_item("size", vm.ctx.new_int(buffer.len()).into(), vm)?;
    
    Ok(dict.into())
}

pub fn create_py_frame_state(vm: &VirtualMachine, frame: &mut FrameState) -> PyResult {
    let dict = vm.ctx.new_dict();
    
    let shape = frame.shape();
    dict.set_item("width", vm.ctx.new_int(shape[1]).into(), vm)?;
    dict.set_item("height", vm.ctx.new_int(shape[0]).into(), vm)?;
    
    // Create the array object
    let array = create_py_array(vm, frame)?;
    dict.set_item("array", array, vm)?;
    
    Ok(dict.into())
}

/// Update the frame object with current engine state
pub fn update_py_frame_state(vm: &VirtualMachine, frame_obj: PyObjectRef, frame: &mut FrameState) -> PyResult<()> {
    // Get the array from the frame object
    let array_obj = vm.get_attribute_opt(frame_obj.clone(), "array")?
        .ok_or_else(|| vm.new_attribute_error("array not found".to_string()))?;
    
    // Update the array's data
    let buffer = frame.buffer_mut();
    let py_buffer: Vec<PyObjectRef> = buffer.iter().map(|&b| vm.ctx.new_int(b).into()).collect();
    let new_list = vm.ctx.new_list(py_buffer);
    
    // Update the data field
    let array_dict = array_obj;
    array_dict.set_attr("data", new_list, vm)?;
    
    Ok(())
}

