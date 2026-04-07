//! xos.tensor API functions exposed to Python.

use crate::python_api::dtypes::DType;
pub use crate::tensor::tensor::{
    PyTensor, create_tensor_from_data, py_number_to_f64, tensor_flat_data_list, tensor_shape_tuple,
};
use rustpython_vm::{PyRef, PyResult, VirtualMachine, builtins::PyModule, function::FuncArgs};

fn wrap_tensor_dict(dict: rustpython_vm::PyObjectRef, vm: &VirtualMachine) -> PyResult {
    if let Ok(wrapper_class) = vm.builtins.get_attr("_TensorWrapper", vm) {
        if let Ok(wrapped) = wrapper_class.call((dict.clone(),), vm) {
            return Ok(wrapped);
        }
    }
    Ok(dict)
}

fn where_fn(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() < 3 {
        return Err(vm.new_type_error("where() requires cond, x, y".to_string()));
    }
    let c = tensor_flat_data_list(&args_vec[0], vm)?;
    let x = tensor_flat_data_list(&args_vec[1], vm)?;
    let y = tensor_flat_data_list(&args_vec[2], vm)?;
    if c.len() != x.len() || x.len() != y.len() {
        return Err(vm.new_value_error("where(): shape mismatch".to_string()));
    }
    let shape = tensor_shape_tuple(&args_vec[1], vm)?;
    let out: Vec<f32> = c
        .iter()
        .zip(x.iter())
        .zip(y.iter())
        .map(|((&cv, &xv), &yv)| if cv != 0.0 { xv } else { yv })
        .collect();
    let dtype = DType::Float32;
    let py_tensor = create_tensor_from_data(out, shape, dtype);
    wrap_tensor_dict(py_tensor.to_py_dict(vm, dtype)?, vm)
}

fn clip_fn(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() < 3 {
        return Err(vm.new_type_error("clip() requires x, min, max".to_string()));
    }
    let a = tensor_flat_data_list(&args_vec[0], vm)?;
    let lo = tensor_flat_data_list(&args_vec[1], vm)?;
    let hi = tensor_flat_data_list(&args_vec[2], vm)?;
    let shape = tensor_shape_tuple(&args_vec[0], vm)?;
    let n = a.len();
    let out = if lo.len() == n && hi.len() == n {
        a.iter()
            .zip(lo.iter())
            .zip(hi.iter())
            .map(|((&x, &l), &h)| x.max(l).min(h))
            .collect()
    } else if n % 2 == 0 && lo.len() * 2 == n && hi.len() * 2 == n {
        let rows = n / 2;
        let mut v = Vec::with_capacity(n);
        for i in 0..rows {
            let l = lo[i];
            let h = hi[i];
            v.push(a[2 * i].max(l).min(h));
            v.push(a[2 * i + 1].max(l).min(h));
        }
        v
    } else {
        return Err(vm.new_value_error("clip(): incompatible shapes".to_string()));
    };
    let dtype = DType::Float32;
    let py_tensor = create_tensor_from_data(out, shape, dtype);
    wrap_tensor_dict(py_tensor.to_py_dict(vm, dtype)?, vm)
}

pub fn tensor_fn(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
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
    fn flatten_list(obj: &rustpython_vm::PyObjectRef, flat: &mut Vec<f32>, vm: &VirtualMachine) -> PyResult<()> {
        if let Some(list) = obj.downcast_ref::<rustpython_vm::builtins::PyList>() {
            for item in list.borrow_vec().iter() {
                flatten_list(item, flat, vm)?;
            }
        } else {
            flat.push(py_number_to_f64(obj, vm)? as f32);
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
        } else if let Some(shape_list) = shape_arg.downcast_ref::<rustpython_vm::builtins::PyList>() {
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
    let casted_data: Vec<f32> = flat_data.iter().map(|&v| dtype.cast_from_f32(v)).collect();
    let py_tensor = create_tensor_from_data(casted_data, shape, dtype);
    wrap_tensor_dict(py_tensor.to_py_dict(vm, dtype)?, vm)
}

pub fn zeros_fn(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
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
    let py_tensor = create_tensor_from_data(vec![0.0f32; total], shape_arg, dtype);
    wrap_tensor_dict(py_tensor.to_py_dict(vm, dtype)?, vm)
}

pub fn ones_fn(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
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
    let py_tensor = create_tensor_from_data(vec![1.0f32; total], shape_arg, dtype);
    wrap_tensor_dict(py_tensor.to_py_dict(vm, dtype)?, vm)
}

pub fn full_fn(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
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
    let py_tensor = create_tensor_from_data(vec![fill_value; total], shape_arg, dtype);
    wrap_tensor_dict(py_tensor.to_py_dict(vm, dtype)?, vm)
}

pub fn arange_fn(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
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
    let py_tensor = create_tensor_from_data(data.clone(), vec![data.len()], dtype);
    wrap_tensor_dict(py_tensor.to_py_dict(vm, dtype)?, vm)
}

pub fn stack_fn(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
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
        rows.push(tensor_flat_data_list(t, vm)?);
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
    wrap_tensor_dict(py_tensor.to_py_dict(vm, dtype)?, vm)
}

pub fn register_tensors_functions(module: &PyRef<PyModule>, vm: &VirtualMachine) {
    module.set_attr("tensor", vm.new_function("tensor", tensor_fn), vm).unwrap();
    module.set_attr("zeros", vm.new_function("zeros", zeros_fn), vm).unwrap();
    module.set_attr("ones", vm.new_function("ones", ones_fn), vm).unwrap();
    module.set_attr("full", vm.new_function("full", full_fn), vm).unwrap();
    module.set_attr("arange", vm.new_function("arange", arange_fn), vm).unwrap();
    module.set_attr("stack", vm.new_function("stack", stack_fn), vm).unwrap();
    module.set_attr("where", vm.new_function("where", where_fn), vm).unwrap();
    module.set_attr("clip", vm.new_function("clip", clip_fn), vm).unwrap();
}