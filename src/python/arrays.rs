use rustpython_vm::{PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs, PyObjectRef};
use crate::tensor::array::{Array, Device};
use std::sync::{Arc, Mutex};

/// Python wrapper for Rust Array<f32>
/// This allows Python code to work with Rust arrays directly, like PyTorch/NumPy
#[derive(Debug, Clone)]
pub struct PyArray {
    // Use Arc<Mutex<>> to allow shared mutable access from Python
    pub data: Arc<Mutex<Array<f32>>>,
}

impl PyArray {
    pub fn new(array: Array<f32>) -> Self {
        Self {
            data: Arc::new(Mutex::new(array)),
        }
    }
    
    pub fn to_py_dict(&self, vm: &VirtualMachine) -> PyResult {
        let array = self.data.lock().unwrap();
        let shape = array.shape();
        let data = array.data();
        
        let dict = vm.ctx.new_dict();
        
        // Add shape as tuple
        dict.set_item("shape", 
            vm.ctx.new_tuple(shape.iter().map(|&s| vm.ctx.new_int(s).into()).collect()).into(), 
            vm)?;
        
        // Add dtype
        dict.set_item("dtype", vm.ctx.new_str("float32").into(), vm)?;
        
        // Add device
        dict.set_item("device", vm.ctx.new_str("cpu").into(), vm)?;
        
        // Store the PyArray wrapper itself so we can modify it
        // For now, we'll create a Python list view of the data
        let py_data: Vec<PyObjectRef> = data.iter().map(|&f| vm.ctx.new_float(f as f64).into()).collect();
        dict.set_item("_data", vm.ctx.new_list(py_data).into(), vm)?;
        
        // Store reference to self
        dict.set_item("_rust_array", vm.ctx.new_int(self.data.as_ref() as *const _ as i64).into(), vm)?;
        
        Ok(dict.into())
    }
}

/// xos.array(data, shape=None) - create a Rust-backed array
fn array(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.is_empty() {
        return Err(vm.new_type_error("array() requires at least 1 argument".to_string()));
    }
    
    let data_arg = &args_vec[0];
    
    // Parse data as list of floats
    let data_list = data_arg.downcast_ref::<rustpython_vm::builtins::PyList>()
        .ok_or_else(|| vm.new_type_error("data must be a list".to_string()))?;
    
    let data_vec = data_list.borrow_vec();
    let mut flat_data = Vec::new();
    
    // Flatten nested lists
    fn flatten_list(obj: &PyObjectRef, flat: &mut Vec<f32>, vm: &VirtualMachine) -> PyResult<()> {
        if let Some(list) = obj.downcast_ref::<rustpython_vm::builtins::PyList>() {
            for item in list.borrow_vec().iter() {
                flatten_list(item, flat, vm)?;
            }
        } else {
            let val: f64 = obj.clone().try_into_value(vm)?;
            flat.push(val as f32);
        }
        Ok(())
    }
    
    for item in data_vec.iter() {
        flatten_list(item, &mut flat_data, vm)?;
    }
    
    // Determine shape
    let shape = if args_vec.len() > 1 {
        // Shape provided as second argument
        let shape_arg = &args_vec[1];
        if let Some(shape_tuple) = shape_arg.downcast_ref::<rustpython_vm::builtins::PyTuple>() {
            shape_tuple.as_slice().iter()
                .map(|s| s.clone().try_into_value::<i32>(vm).map(|i| i as usize))
                .collect::<Result<Vec<_>, _>>()?
        } else if let Some(shape_list) = shape_arg.downcast_ref::<rustpython_vm::builtins::PyList>() {
            shape_list.borrow_vec().iter()
                .map(|s| s.clone().try_into_value::<i32>(vm).map(|i| i as usize))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            vec![flat_data.len()]
        }
    } else {
        vec![flat_data.len()]
    };
    
    // Create Rust array on CPU (for Python array manipulation)
    // When we move physics to Rust, we can use Metal device
    let rust_array = Array::new_on_device(flat_data, shape, Device::Cpu);
    let py_array = PyArray::new(rust_array);
    
    let dict = py_array.to_py_dict(vm)?;
    
    // Wrap in _ArrayWrapper for nice display
    if let Ok(wrapper_class) = vm.builtins.get_attr("_ArrayWrapper", vm) {
        if let Ok(wrapped) = wrapper_class.call((dict.clone(),), vm) {
            return Ok(wrapped);
        }
    }
    
    Ok(dict)
}

/// xos.zeros(shape) - create array filled with zeros
fn zeros(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let shape_arg: Vec<usize> = args.bind(vm)?;
    let total: usize = shape_arg.iter().product();
    let data = vec![0.0f32; total];
    // Create on CPU for Python manipulation
    let rust_array = Array::new_on_device(data, shape_arg, Device::Cpu);
    let py_array = PyArray::new(rust_array);
    let dict = py_array.to_py_dict(vm)?;
    
    // Wrap in _ArrayWrapper for nice display
    if let Ok(wrapper_class) = vm.builtins.get_attr("_ArrayWrapper", vm) {
        if let Ok(wrapped) = wrapper_class.call((dict.clone(),), vm) {
            return Ok(wrapped);
        }
    }
    
    Ok(dict)
}

pub fn make_arrays_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.arrays", vm.ctx.new_dict(), None);
    module.set_attr("array", vm.new_function("array", array), vm).unwrap();
    module.set_attr("zeros", vm.new_function("zeros", zeros), vm).unwrap();
    module
}

