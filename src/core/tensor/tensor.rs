//! Python-facing tensor helpers and constructors.
//!
//! This module currently keeps Python tensor compatibility (`_data`, `shape`) and is designed
//! so we can swap internal storage to Burn-backed tensors incrementally.

use crate::python_api::dtypes::DType;
use rustpython_vm::{
    PyObjectRef, PyResult, VirtualMachine, builtins::PyDict, builtins::PyList, builtins::PyTuple,
};
use std::sync::{Arc, Mutex};

/// Python wrapper for tensor - stores f32 data with shape.
/// Backed by Vec<f32> for Python compatibility; Burn tensors are introduced incrementally.
#[derive(Clone)]
pub struct PyTensor {
    pub data: Arc<Mutex<Vec<f32>>>,
    pub shape: Vec<usize>,
}

impl PyTensor {
    pub fn new(data: Vec<f32>, shape: Vec<usize>) -> Self {
        Self {
            data: Arc::new(Mutex::new(data)),
            shape,
        }
    }

    pub fn to_py_dict(&self, vm: &VirtualMachine, dtype: DType) -> PyResult<PyObjectRef> {
        let data_guard = self.data.lock().unwrap();
        let shape = &self.shape;

        let dict = vm.ctx.new_dict();

        dict.set_item(
            "shape",
            vm.ctx
                .new_tuple(shape.iter().map(|&s| vm.ctx.new_int(s).into()).collect())
                .into(),
            vm,
        )?;

        dict.set_item("dtype", vm.ctx.new_str(dtype.name()).into(), vm)?;
        dict.set_item("device", vm.ctx.new_str("cpu").into(), vm)?;

        let py_data: Vec<PyObjectRef> = data_guard
            .iter()
            .map(|&f| {
                let casted = dtype.cast_from_f32(f);
                if dtype.is_float() {
                    vm.ctx.new_float(casted as f64).into()
                } else {
                    vm.ctx.new_int(casted as i64).into()
                }
            })
            .collect();
        dict.set_item("_data", vm.ctx.new_list(py_data).into(), vm)?;
        dict.set_item(
            "_rust_tensor",
            vm.ctx.new_int(self.data.as_ref() as *const _ as i64).into(),
            vm,
        )?;

        Ok(dict.into())
    }
}

/// Extract f64 from Python int or float.
pub fn py_number_to_f64(obj: &PyObjectRef, vm: &VirtualMachine) -> PyResult<f64> {
    if let Ok(f) = obj.clone().try_into_value::<f64>(vm) {
        return Ok(f);
    }
    if let Ok(i) = obj.clone().try_into_value::<i64>(vm) {
        return Ok(i as f64);
    }
    Err(vm.new_type_error("Expected a number (int or float)".to_string()))
}

/// Resolve raw tensor dict, `_TensorWrapper`, or nested `_data` to the flat `PyList` of values.
pub fn tensor_flat_data_list(obj: &PyObjectRef, vm: &VirtualMachine) -> PyResult<Vec<f32>> {
    let mut cur = obj.clone();
    for _ in 0..8 {
        if let Some(list) = cur.downcast_ref::<PyList>() {
            return list
                .borrow_vec()
                .iter()
                .map(|x| py_number_to_f64(x, vm).map(|v| v as f32))
                .collect::<Result<Vec<f32>, _>>();
        }
        if let Some(dict) = cur.downcast_ref::<PyDict>() {
            if let Ok(item) = dict.get_item("_data", vm) {
                cur = item;
                continue;
            }
        }
        if let Ok(Some(attr)) = vm.get_attribute_opt(cur.clone(), "_data") {
            cur = attr;
            continue;
        }
        break;
    }
    Err(vm.new_type_error("tensor missing _data list".to_string()))
}

pub fn tensor_shape_tuple(obj: &PyObjectRef, vm: &VirtualMachine) -> PyResult<Vec<usize>> {
    let mut cur = obj.clone();
    for _ in 0..8 {
        if let Some(dict) = cur.downcast_ref::<PyDict>() {
            if let Ok(shape_obj) = dict.get_item("shape", vm) {
                if let Some(tup) = shape_obj.downcast_ref::<PyTuple>() {
                    return tup
                        .as_slice()
                        .iter()
                        .map(|s| s.clone().try_into_value::<i32>(vm).map(|i| i as usize))
                        .collect::<Result<Vec<_>, _>>();
                }
            }
        }
        if let Ok(Some(attr)) = vm.get_attribute_opt(cur.clone(), "shape") {
            cur = attr;
            if let Some(tup) = cur.downcast_ref::<PyTuple>() {
                return tup
                    .as_slice()
                    .iter()
                    .map(|s| s.clone().try_into_value::<i32>(vm).map(|i| i as usize))
                    .collect::<Result<Vec<_>, _>>();
            }
        }
        break;
    }
    Err(vm.new_type_error("tensor missing shape".to_string()))
}

/// Create tensor from flat data and shape.
pub fn create_tensor_from_data(flat_data: Vec<f32>, shape: Vec<usize>, _dtype: DType) -> PyTensor {
    PyTensor::new(flat_data, shape)
}
