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
    let shape = frame.shape();
    let buffer = frame.buffer_mut();
    
    // Tensor metadata dict (CPU RGBA; rasterizer writes the real buffer directly)
    let tensor_dict = vm.ctx.new_dict();
    tensor_dict.set_item("shape", vm.ctx.new_tuple(shape.iter().map(|&s| vm.ctx.new_int(s).into()).collect()).into(), vm)?;
    tensor_dict.set_item("device", vm.ctx.new_str("cpu").into(), vm)?;
    
    // Create a Python list that directly wraps the buffer
    let py_buffer: Vec<PyObjectRef> = buffer.iter().map(|&b| vm.ctx.new_int(b).into()).collect();
    tensor_dict.set_item("data", vm.ctx.new_list(py_buffer).into(), vm)?;
    
    tensor_dict.set_item("dtype", vm.ctx.new_str("uint8").into(), vm)?;
    tensor_dict.set_item("size", vm.ctx.new_int(buffer.len()).into(), vm)?;
    
    // Create frame dict
    let frame_dict = vm.ctx.new_dict();
    frame_dict.set_item("width", vm.ctx.new_int(shape[1]).into(), vm)?;
    frame_dict.set_item("height", vm.ctx.new_int(shape[0]).into(), vm)?;
    frame_dict.set_item("tensor", tensor_dict.into(), vm)?;
    let s = &frame.safe_region_boundaries;
    frame_dict.set_item("safe_x1", vm.ctx.new_float(f64::from(s.x1)).into(), vm)?;
    frame_dict.set_item("safe_y1", vm.ctx.new_float(f64::from(s.y1)).into(), vm)?;
    frame_dict.set_item("safe_x2", vm.ctx.new_float(f64::from(s.x2)).into(), vm)?;
    frame_dict.set_item("safe_y2", vm.ctx.new_float(f64::from(s.y2)).into(), vm)?;

    Ok(frame_dict.into())
}

/// Update the frame object with current engine state
/// This copies the Rust buffer data back to Python after rendering
pub fn update_py_frame_state(vm: &VirtualMachine, frame_obj: PyObjectRef, frame: &mut FrameState) -> PyResult<()> {
    // frame_obj might be a _FrameWrapper, get the underlying dict
    let actual_dict = if let Ok(data_attr) = vm.get_attribute_opt(frame_obj.clone(), "_data") {
        if let Some(data) = data_attr {
            data
        } else {
            frame_obj.clone()
        }
    } else {
        frame_obj.clone()
    };
    
    let frame_dict = actual_dict.downcast_ref::<rustpython_vm::builtins::PyDict>()
        .ok_or_else(|| vm.new_type_error("frame is not a dict".to_string()))?;
    
    // Get the tensor metadata dict from the frame
    let tensor_obj = frame_dict.get_item("tensor", vm)?;
    
    let tensor_dict = tensor_obj.downcast_ref::<rustpython_vm::builtins::PyDict>()
        .ok_or_else(|| vm.new_type_error("tensor is not a dict".to_string()))?;
    
    // Update the tensor's shape (in case of window resize)
    let shape = frame.shape();
    tensor_dict.set_item("shape", vm.ctx.new_tuple(shape.iter().map(|&s| vm.ctx.new_int(s).into()).collect()).into(), vm)?;
    
    // DON'T copy the entire buffer to Python - that's millions of allocations!
    // Instead, store a pointer that the rasterizer can use directly
    // For now, just update metadata - the rasterizer will access Rust buffer directly
    
    // Also update the frame dict's width and height
    let width = shape[1];
    let height = shape[0];
    frame_dict.set_item("width", vm.ctx.new_int(width).into(), vm)?;
    frame_dict.set_item("height", vm.ctx.new_int(height).into(), vm)?;
    let s = &frame.safe_region_boundaries;
    frame_dict.set_item("safe_x1", vm.ctx.new_float(f64::from(s.x1)).into(), vm)?;
    frame_dict.set_item("safe_y1", vm.ctx.new_float(f64::from(s.y1)).into(), vm)?;
    frame_dict.set_item("safe_x2", vm.ctx.new_float(f64::from(s.x2)).into(), vm)?;
    frame_dict.set_item("safe_y2", vm.ctx.new_float(f64::from(s.y2)).into(), vm)?;

    Ok(())
}

/// Sync Python buffer changes back to Rust
/// Since we no longer copy the buffer to Python, this is a no-op
pub fn sync_py_buffer_to_rust(_vm: &VirtualMachine, _frame_obj: PyObjectRef, _frame: &mut FrameState) -> PyResult<()> {
    // No-op: rasterizer writes directly to Rust buffer
    Ok(())
}

