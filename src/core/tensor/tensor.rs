//! Python-facing tensor helpers and constructors.
//!
//! This module currently keeps Python tensor compatibility (`_data`, `shape`) and is designed
//! so we can swap internal storage to Burn-backed tensors incrementally.

use crate::python_api::dtypes::DType;
use once_cell::sync::Lazy;
use rustpython_vm::{
    PyObjectRef, PyResult, VirtualMachine, builtins::PyDict, builtins::PyList, builtins::PyTuple,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

static NEXT_TENSOR_ID: AtomicU64 = AtomicU64::new(1);
static TENSOR_REGISTRY: Lazy<Mutex<HashMap<u64, Arc<Mutex<Vec<f32>>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// CPU-side tensor storage for the Python API (`shape`, `dtype`, `device` in the emitted dict).
/// GPU-backed paths use the same [`Tensor`] name with `device` set accordingly elsewhere.
#[derive(Clone)]
pub struct Tensor {
    pub id: u64,
    pub data: Arc<Mutex<Vec<f32>>>,
    pub shape: Vec<usize>,
}

impl Tensor {
    pub fn new(data: Vec<f32>, shape: Vec<usize>) -> Self {
        let id = NEXT_TENSOR_ID.fetch_add(1, Ordering::Relaxed);
        let data = Arc::new(Mutex::new(data));
        if let Ok(mut reg) = TENSOR_REGISTRY.lock() {
            reg.insert(id, data.clone());
        }
        Self {
            id,
            data,
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
            vm.ctx.new_int(self.id as i64).into(),
            vm,
        )?;

        Ok(dict.into())
    }
}

fn try_get_tensor_data_by_id(id: u64) -> Option<Vec<f32>> {
    let reg = TENSOR_REGISTRY.lock().ok()?;
    let data = reg.get(&id)?.clone();
    drop(reg);
    let guard = data.lock().ok()?;
    Some(guard.clone())
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

/// Resolve raw tensor dict, Python `Tensor` wrapper, or nested `_data` to the flat `PyList` of values.
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
            if let Ok(id_obj) = dict.get_item("_rust_tensor", vm) {
                if let Ok(id) = id_obj.try_into_value::<i64>(vm) {
                    if let Some(v) = try_get_tensor_data_by_id(id.max(0) as u64) {
                        return Ok(v);
                    }
                }
            }
            if let Ok(item) = dict.get_item("_data", vm) {
                cur = item;
                continue;
            }
            if let Ok(item) = dict.get_item("data", vm) {
                cur = item;
                continue;
            }
            if let Ok(vid_obj) = dict.get_item("_xos_viewport_id", vm) {
                if let Ok(vid) = vid_obj.try_into_value::<i64>(vm) {
                    if let Some(bytes) = crate::python_api::xos_module::standalone_frame_buffer_copy(vid.max(0) as u64) {
                        let out = bytes.into_iter().map(|b| b as f32).collect::<Vec<f32>>();
                        return Ok(out);
                    }
                }
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
pub fn create_tensor_from_data(flat_data: Vec<f32>, shape: Vec<usize>, _dtype: DType) -> Tensor {
    Tensor::new(flat_data, shape)
}
