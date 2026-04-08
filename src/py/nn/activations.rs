use rustpython_vm::{PyResult, VirtualMachine, function::FuncArgs};

fn relu_forward(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() < 2 {
        return Err(vm.new_type_error("ReLU.forward() requires input tensor".to_string()));
    }

    let x = crate::python_api::tensors::tensor_flat_data_list(&args_vec[1], vm)?;
    let shape = crate::python_api::tensors::tensor_shape_tuple(&args_vec[1], vm)?;
    let out: Vec<f32> = x.into_iter().map(|v| v.max(0.0)).collect();
    let dtype = crate::python_api::dtypes::DType::Float32;
    let py_tensor = crate::python_api::tensors::create_tensor_from_data(out, shape, dtype);
    let dict = py_tensor.to_py_dict(vm, dtype)?;
    if let Ok(wrapper_class) = vm.builtins.get_attr("Tensor", vm) {
        if let Ok(wrapped) = wrapper_class.call((dict.clone(),), vm) {
            return Ok(wrapped);
        }
    }
    Ok(dict)
}

fn relu_call(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    relu_forward(args, vm)
}

pub fn relu_new(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let obj = vm.ctx.new_dict();
    obj.set_item("__class_name__", vm.ctx.new_str("ReLU").into(), vm)?;
    obj.set_item("forward", vm.new_function("forward", relu_forward).into(), vm)?;
    obj.set_item("__call__", vm.new_function("__call__", relu_call).into(), vm)?;
    Ok(obj.into())
}
