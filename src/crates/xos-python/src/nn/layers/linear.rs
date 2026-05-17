use crate::dtypes::DType;
use crate::tensors::{create_tensor_from_data, tensor_flat_data_list};
use rustpython_vm::{
    builtins::PyDict, builtins::PyList, builtins::PyTuple, function::FuncArgs, PyResult,
    VirtualMachine,
};

use super::wrap_tensor_dict;

fn linear_forward(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() < 2 {
        return Err(vm.new_type_error("Linear.forward() requires input tensor".to_string()));
    }
    let self_obj = &args_vec[0];
    let input_obj = &args_vec[1];

    let (out_features, shape) = if let Some(dict) = self_obj.downcast_ref::<PyDict>() {
        if let Ok(v) = dict.get_item("out_features", vm) {
            if let Some(tup) = v.downcast_ref::<PyTuple>() {
                let mut dims = Vec::with_capacity(tup.as_slice().len() + 1);
                dims.push(1usize);
                for d in tup.as_slice().iter() {
                    let parsed = d.clone().try_into_value::<i32>(vm).unwrap_or(1).max(1) as usize;
                    dims.push(parsed);
                }
                let out = dims[1..].iter().copied().product::<usize>().max(1);
                (out, dims)
            } else if let Some(lst) = v.downcast_ref::<PyList>() {
                let items = lst.borrow_vec();
                let mut dims = Vec::with_capacity(items.len() + 1);
                dims.push(1usize);
                for d in items.iter() {
                    let parsed = d.clone().try_into_value::<i32>(vm).unwrap_or(1).max(1) as usize;
                    dims.push(parsed);
                }
                let out = dims[1..].iter().copied().product::<usize>().max(1);
                (out, dims)
            } else {
                let out = v.try_into_value::<i32>(vm).unwrap_or(1).max(1) as usize;
                (out, vec![1, out])
            }
        } else {
            (1, vec![1, 1])
        }
    } else {
        (1, vec![1, 1])
    };

    let src = tensor_flat_data_list(input_obj, vm).unwrap_or_else(|_| vec![0.0]);
    let src = if src.is_empty() { vec![0.0] } else { src };

    // Deterministic placeholder projection: repeat/truncate source values.
    let mut flat = Vec::with_capacity(out_features);
    for i in 0..out_features {
        flat.push(src[i % src.len()]);
    }

    let py_tensor = create_tensor_from_data(flat, shape, DType::Float32);
    wrap_tensor_dict(py_tensor.to_py_dict(vm, DType::Float32)?, vm)
}

fn linear_call(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    linear_forward(args, vm)
}

pub fn linear_new(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let obj = vm.ctx.new_dict();
    obj.set_item("__class_name__", vm.ctx.new_str("Linear").into(), vm)?;
    if let Some(v) = args.kwargs.get("in_features") {
        obj.set_item("in_features", v.clone(), vm)?;
    }
    if let Some(v) = args.kwargs.get("out_features") {
        obj.set_item("out_features", v.clone(), vm)?;
    }
    if let Some(v) = args.kwargs.get("bias") {
        obj.set_item("bias", v.clone(), vm)?;
    }
    obj.set_item(
        "forward",
        vm.new_function("forward", linear_forward).into(),
        vm,
    )?;
    obj.set_item(
        "__call__",
        vm.new_function("__call__", linear_call).into(),
        vm,
    )?;
    Ok(obj.into())
}
