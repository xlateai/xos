//! xos.tensor - Burn-backed tensors exposed to Python with same API as xos.array

use rustpython_vm::{
    PyResult, VirtualMachine, builtins::PyDict, builtins::PyList, builtins::PyModule, PyRef,
    function::FuncArgs, PyObjectRef,
};
use crate::python_api::dtypes::DType;
use std::sync::{Arc, Mutex};

/// Python wrapper for tensor - stores f32 data with shape
/// Backed by Vec<f32> for Python compatibility; Burn tensors used for ML ops
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

/// Extract f64 from Python int or float (handles "expected float but got int" from strict conversion)
fn py_number_to_f64(obj: &PyObjectRef, vm: &VirtualMachine) -> PyResult<f64> {
    if let Ok(f) = obj.clone().try_into_value::<f64>(vm) {
        return Ok(f);
    }
    if let Ok(i) = obj.clone().try_into_value::<i64>(vm) {
        return Ok(i as f64);
    }
    Err(vm.new_type_error("Expected a number (int or float)".to_string()))
}

/// Resolve raw tensor dict, `_ArrayWrapper`, or nested `_data` to the flat `PyList` of values.
fn tensor_flat_data_list(obj: &PyObjectRef, vm: &VirtualMachine) -> PyResult<Vec<f32>> {
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

/// Create Burn tensor from flat f32 data and shape, return as PyTensor
fn create_tensor_from_data(
    flat_data: Vec<f32>,
    shape: Vec<usize>,
    _dtype: DType,
) -> PyTensor {
    // Store as vec - Burn tensor would require reshape which needs const D
    // For now keep it simple: we use Vec<f32> as backing, Burn can be added for ops
    PyTensor::new(flat_data, shape)
}

/// xos.tensor(data, shape=None, dtype=None) - create Burn-backed tensor
fn tensor_fn(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.is_empty() {
        return Err(vm.new_type_error("tensor() requires at least 1 argument".to_string()));
    }

    let data_arg = &args_vec[0];

    let dtype = if args_vec.len() > 2 && !vm.is_none(&args_vec[2]) {
        DType::from_py_object(&args_vec[2], vm).unwrap_or(DType::Float32)
    } else if let Some(dtype_kwarg) = args.kwargs.get("dtype") {
        DType::from_py_object(dtype_kwarg, vm).unwrap_or(DType::Float32)
    } else {
        DType::Float32
    };

    let data_list = data_arg
        .downcast_ref::<rustpython_vm::builtins::PyList>()
        .ok_or_else(|| vm.new_type_error("data must be a list".to_string()))?;

    let data_vec = data_list.borrow_vec();
    let mut flat_data = Vec::new();

    fn flatten_list(obj: &PyObjectRef, flat: &mut Vec<f32>, vm: &VirtualMachine) -> PyResult<()> {
        if let Some(list) = obj.downcast_ref::<rustpython_vm::builtins::PyList>() {
            for item in list.borrow_vec().iter() {
                flatten_list(item, flat, vm)?;
            }
        } else {
            let val = py_number_to_f64(obj, vm)?;
            flat.push(val as f32);
        }
        Ok(())
    }

    for item in data_vec.iter() {
        flatten_list(item, &mut flat_data, vm)?;
    }

    let shape = if args_vec.len() > 1 {
        let shape_arg = &args_vec[1];
        if let Some(shape_tuple) = shape_arg.downcast_ref::<rustpython_vm::builtins::PyTuple>() {
            shape_tuple
                .as_slice()
                .iter()
                .map(|s| s.clone().try_into_value::<i32>(vm).map(|i| i as usize))
                .collect::<Result<Vec<_>, _>>()?
        } else if let Some(shape_list) = shape_arg.downcast_ref::<rustpython_vm::builtins::PyList>()
        {
            shape_list
                .borrow_vec()
                .iter()
                .map(|s| s.clone().try_into_value::<i32>(vm).map(|i| i as usize))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            vec![flat_data.len()]
        }
    } else {
        vec![flat_data.len()]
    };

    let casted_data: Vec<f32> = flat_data
        .iter()
        .map(|&v| dtype.cast_from_f32(v))
        .collect();

    let py_tensor = create_tensor_from_data(casted_data, shape, dtype);
    let dict = py_tensor.to_py_dict(vm, dtype)?;

    if let Ok(wrapper_class) = vm.builtins.get_attr("_ArrayWrapper", vm) {
        if let Ok(wrapped) = wrapper_class.call((dict.clone(),), vm) {
            return Ok(wrapped);
        }
    }

    Ok(dict)
}

/// xos.zeros(shape, dtype=float32)
fn zeros_fn(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.is_empty() {
        return Err(vm.new_type_error("zeros() requires 1 argument (shape)".to_string()));
    }

    let shape_arg: Vec<usize> = args_vec[0]
        .downcast_ref::<rustpython_vm::builtins::PyTuple>()
        .ok_or_else(|| vm.new_type_error("shape must be a tuple".to_string()))?
        .as_slice()
        .iter()
        .map(|s| s.clone().try_into_value::<i32>(vm).map(|i| i as usize))
        .collect::<Result<Vec<_>, _>>()?;

    let dtype = if args_vec.len() > 1 && !vm.is_none(&args_vec[1]) {
        DType::from_py_object(&args_vec[1], vm).unwrap_or(DType::Float32)
    } else if let Some(dtype_kwarg) = args.kwargs.get("dtype") {
        DType::from_py_object(dtype_kwarg, vm).unwrap_or(DType::Float32)
    } else {
        DType::Float32
    };

    let total: usize = shape_arg.iter().product();
    let data = vec![0.0f32; total];
    let py_tensor = create_tensor_from_data(data, shape_arg, dtype);
    let dict = py_tensor.to_py_dict(vm, dtype)?;

    if let Ok(wrapper_class) = vm.builtins.get_attr("_ArrayWrapper", vm) {
        if let Ok(wrapped) = wrapper_class.call((dict.clone(),), vm) {
            return Ok(wrapped);
        }
    }

    Ok(dict)
}

/// xos.ones(shape, dtype=float32)
fn ones_fn(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.is_empty() {
        return Err(vm.new_type_error("ones() requires 1 argument (shape)".to_string()));
    }

    let shape_arg: Vec<usize> = args_vec[0]
        .downcast_ref::<rustpython_vm::builtins::PyTuple>()
        .ok_or_else(|| vm.new_type_error("shape must be a tuple".to_string()))?
        .as_slice()
        .iter()
        .map(|s| s.clone().try_into_value::<i32>(vm).map(|i| i as usize))
        .collect::<Result<Vec<_>, _>>()?;

    let dtype = if args_vec.len() > 1 && !vm.is_none(&args_vec[1]) {
        DType::from_py_object(&args_vec[1], vm).unwrap_or(DType::Float32)
    } else if let Some(dtype_kwarg) = args.kwargs.get("dtype") {
        DType::from_py_object(dtype_kwarg, vm).unwrap_or(DType::Float32)
    } else {
        DType::Float32
    };

    let total: usize = shape_arg.iter().product();
    let data = vec![1.0f32; total];
    let py_tensor = create_tensor_from_data(data, shape_arg, dtype);
    let dict = py_tensor.to_py_dict(vm, dtype)?;

    if let Ok(wrapper_class) = vm.builtins.get_attr("_ArrayWrapper", vm) {
        if let Ok(wrapped) = wrapper_class.call((dict.clone(),), vm) {
            return Ok(wrapped);
        }
    }

    Ok(dict)
}

/// xos.full(shape, value, dtype=float32)
fn full_fn(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() < 2 {
        return Err(vm.new_type_error("full() requires shape and fill value".to_string()));
    }

    let shape_arg: Vec<usize> = args_vec[0]
        .downcast_ref::<rustpython_vm::builtins::PyTuple>()
        .ok_or_else(|| vm.new_type_error("shape must be a tuple".to_string()))?
        .as_slice()
        .iter()
        .map(|s| s.clone().try_into_value::<i32>(vm).map(|i| i as usize))
        .collect::<Result<Vec<_>, _>>()?;

    let fill_value = py_number_to_f64(&args_vec[1], vm)? as f32;
    let dtype = if args_vec.len() > 2 && !vm.is_none(&args_vec[2]) {
        DType::from_py_object(&args_vec[2], vm).unwrap_or(DType::Float32)
    } else if let Some(dtype_kwarg) = args.kwargs.get("dtype") {
        DType::from_py_object(dtype_kwarg, vm).unwrap_or(DType::Float32)
    } else {
        DType::Float32
    };

    let total: usize = shape_arg.iter().product();
    let data = vec![fill_value; total];
    let py_tensor = create_tensor_from_data(data, shape_arg, dtype);
    let dict = py_tensor.to_py_dict(vm, dtype)?;

    if let Ok(wrapper_class) = vm.builtins.get_attr("_ArrayWrapper", vm) {
        if let Ok(wrapped) = wrapper_class.call((dict.clone(),), vm) {
            return Ok(wrapped);
        }
    }

    Ok(dict)
}

/// xos.arange(start, stop=None, step=1, dtype=float32)
fn arange_fn(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.is_empty() {
        return Err(vm.new_type_error("arange() requires at least start".to_string()));
    }

    let (start, stop, step) = if args_vec.len() == 1 {
        (0.0, py_number_to_f64(&args_vec[0], vm)?, 1.0)
    } else {
        let start = py_number_to_f64(&args_vec[0], vm)?;
        let stop = py_number_to_f64(&args_vec[1], vm)?;
        let step = if args_vec.len() > 2 {
            py_number_to_f64(&args_vec[2], vm)?
        } else {
            1.0
        };
        (start, stop, step)
    };

    if step == 0.0 {
        return Err(vm.new_value_error("arange() step must not be 0".to_string()));
    }

    let dtype = if args_vec.len() > 3 && !vm.is_none(&args_vec[3]) {
        DType::from_py_object(&args_vec[3], vm).unwrap_or(DType::Float32)
    } else if let Some(dtype_kwarg) = args.kwargs.get("dtype") {
        DType::from_py_object(dtype_kwarg, vm).unwrap_or(DType::Float32)
    } else {
        DType::Float32
    };

    let mut data = Vec::new();
    let mut v = start;
    if step > 0.0 {
        while v < stop {
            data.push(v as f32);
            v += step;
        }
    } else {
        while v > stop {
            data.push(v as f32);
            v += step;
        }
    }

    let shape = vec![data.len()];
    let py_tensor = create_tensor_from_data(data, shape, dtype);
    let dict = py_tensor.to_py_dict(vm, dtype)?;
    if let Ok(wrapper_class) = vm.builtins.get_attr("_ArrayWrapper", vm) {
        if let Ok(wrapped) = wrapper_class.call((dict.clone(),), vm) {
            return Ok(wrapped);
        }
    }
    Ok(dict)
}

/// xos.stack([a, b, ...], axis=0|1) - minimal implementation for 1D inputs
fn stack_fn(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.is_empty() {
        return Err(vm.new_type_error("stack() requires a list of tensors".to_string()));
    }

    let tensors = args_vec[0]
        .downcast_ref::<rustpython_vm::builtins::PyList>()
        .ok_or_else(|| vm.new_type_error("stack() first arg must be a list".to_string()))?;
    let axis = if args_vec.len() > 1 {
        args_vec[1].clone().try_into_value::<i32>(vm).unwrap_or(0)
    } else if let Some(axis_kwarg) = args.kwargs.get("axis") {
        axis_kwarg.clone().try_into_value::<i32>(vm).unwrap_or(0)
    } else {
        0
    };

    let tensor_items = tensors.borrow_vec();
    if tensor_items.is_empty() {
        return Err(vm.new_value_error("stack() requires at least one tensor".to_string()));
    }

    let mut rows: Vec<Vec<f32>> = Vec::new();
    for t in tensor_items.iter() {
        let row = tensor_flat_data_list(t, vm)?;
        rows.push(row);
    }

    let n = rows[0].len();
    if rows.iter().any(|r| r.len() != n) {
        return Err(vm.new_value_error("stack() all tensors must have same length".to_string()));
    }

    let (flat, shape) = if axis == 1 {
        let mut out = vec![0.0f32; n * rows.len()];
        for i in 0..n {
            for j in 0..rows.len() {
                out[i * rows.len() + j] = rows[j][i];
            }
        }
        (out, vec![n, rows.len()])
    } else {
        let mut out = Vec::with_capacity(n * rows.len());
        for row in rows.iter() {
            out.extend_from_slice(row);
        }
        (out, vec![rows.len(), n])
    };

    let dtype = DType::Float32;
    let py_tensor = create_tensor_from_data(flat, shape, dtype);
    let dict = py_tensor.to_py_dict(vm, dtype)?;
    if let Ok(wrapper_class) = vm.builtins.get_attr("_ArrayWrapper", vm) {
        if let Ok(wrapped) = wrapper_class.call((dict.clone(),), vm) {
            return Ok(wrapped);
        }
    }
    Ok(dict)
}

pub fn make_tensors_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.tensors", vm.ctx.new_dict(), None);
    module.set_attr("tensor", vm.new_function("tensor", tensor_fn), vm).unwrap();
    module.set_attr("zeros", vm.new_function("zeros", zeros_fn), vm).unwrap();
    module.set_attr("ones", vm.new_function("ones", ones_fn), vm).unwrap();
    module.set_attr("full", vm.new_function("full", full_fn), vm).unwrap();
    module.set_attr("arange", vm.new_function("arange", arange_fn), vm).unwrap();
    module.set_attr("stack", vm.new_function("stack", stack_fn), vm).unwrap();
    module
}
