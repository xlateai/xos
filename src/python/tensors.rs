//! xos.tensor - Burn-backed tensors exposed to Python with same API as xos.array

use rustpython_vm::{
    PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs, PyObjectRef,
};
use crate::python::dtypes::DType;
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

/// Create Burn tensor from flat f32 data and shape, return as PyTensor
fn create_tensor_from_data(
    flat_data: Vec<f32>,
    shape: Vec<usize>,
    dtype: DType,
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
            let val: f64 = obj.clone().try_into_value(vm)?;
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

pub fn make_tensors_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.tensors", vm.ctx.new_dict(), None);
    module.set_attr("tensor", vm.new_function("tensor", tensor_fn), vm).unwrap();
    module.set_attr("zeros", vm.new_function("zeros", zeros_fn), vm).unwrap();
    module.set_attr("ones", vm.new_function("ones", ones_fn), vm).unwrap();
    module
}
