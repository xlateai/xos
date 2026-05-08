use crate::python_api::dtypes::DType;
use crate::python_api::tensors::{tensor_flat_data_list, tensor_shape_tuple, Tensor};
use rustpython_vm::{builtins::PyModule, function::FuncArgs, PyRef, PyResult, VirtualMachine};

fn py_number_to_f32(
    value: rustpython_vm::PyObjectRef,
    vm: &VirtualMachine,
    name: &str,
) -> PyResult<f32> {
    if let Ok(v) = value.clone().try_into_value::<f64>(vm) {
        return Ok(v as f32);
    }
    if let Ok(v) = value.clone().try_into_value::<i64>(vm) {
        return Ok(v as f32);
    }
    Err(vm.new_type_error(format!("{name} must be int or float")))
}

fn wrap_tensor_dict(dict: rustpython_vm::PyObjectRef, vm: &VirtualMachine) -> PyResult {
    if let Ok(wrapper_class) = vm.builtins.get_attr("Tensor", vm) {
        if let Ok(wrapped) = wrapper_class.call((dict.clone(),), vm) {
            return Ok(wrapped);
        }
    }
    Ok(dict)
}

fn containing(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() != 1 {
        return Err(vm.new_type_error(format!(
            "containing() takes exactly 1 argument ({} given)",
            args_vec.len()
        )));
    }

    let flat = tensor_flat_data_list(&args_vec[0], vm)?;
    let shape = tensor_shape_tuple(&args_vec[0], vm).unwrap_or_default();
    if flat.is_empty() {
        // Graceful empty-case for callers that may have no visible hitboxes.
        let py_tensor = Tensor::new(vec![0.0, 0.0, 0.0, 0.0], vec![2, 2]);
        return wrap_tensor_dict(py_tensor.to_py_dict(vm, DType::Float32)?.into(), vm);
    }
    if flat.len() < 4 {
        return Err(vm.new_type_error(
            "containing(): invalid rect tensor size; expected multiples of 4 values".to_string(),
        ));
    }
    if !(shape == vec![2, 2]
        || (shape.len() == 3 && shape[1] == 2 && shape[2] == 2)
        || flat.len() % 4 == 0)
    {
        return Err(vm.new_type_error(
            "containing(): expected shape (2,2), (N,2,2), or flat length multiple of 4".to_string(),
        ));
    }

    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;

    for chunk in flat.chunks_exact(4) {
        let x1 = chunk[0];
        let y1 = chunk[1];
        let x2 = chunk[2];
        let y2 = chunk[3];
        let xa = x1.min(x2);
        let xb = x1.max(x2);
        let ya = y1.min(y2);
        let yb = y1.max(y2);
        min_x = min_x.min(xa);
        min_y = min_y.min(ya);
        max_x = max_x.max(xb);
        max_y = max_y.max(yb);
    }

    let out = vec![min_x, min_y, max_x, max_y];
    let py_tensor = Tensor::new(out, vec![2, 2]);
    wrap_tensor_dict(py_tensor.to_py_dict(vm, DType::Float32)?.into(), vm)
}

fn buffer(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() != 2 {
        return Err(vm.new_type_error(format!(
            "buffer() takes exactly 2 arguments ({} given)",
            args_vec.len()
        )));
    }

    let flat = tensor_flat_data_list(&args_vec[0], vm)?;
    let shape = tensor_shape_tuple(&args_vec[0], vm).unwrap_or_default();
    if flat.is_empty() {
        let out_shape = if shape == vec![2, 2] {
            vec![2, 2]
        } else if shape.len() == 3 && shape[1] == 2 && shape[2] == 2 {
            vec![shape[0], 2, 2]
        } else {
            vec![0, 2, 2]
        };
        let py_tensor = Tensor::new(Vec::new(), out_shape);
        return wrap_tensor_dict(py_tensor.to_py_dict(vm, DType::Float32)?.into(), vm);
    }
    if !(shape == vec![2, 2]
        || (shape.len() == 3 && shape[1] == 2 && shape[2] == 2)
        || flat.len() % 4 == 0)
    {
        return Err(vm.new_type_error(
            "buffer(): expected shape (2,2), (N,2,2), or flat length multiple of 4".to_string(),
        ));
    }
    let scale: f32 = py_number_to_f32(args_vec[1].clone(), vm, "scale")?;
    if !scale.is_finite() || scale <= 0.0 {
        return Err(vm.new_value_error("buffer(): scale must be > 0".to_string()));
    }

    let mut out = Vec::with_capacity(flat.len());
    for chunk in flat.chunks_exact(4) {
        let x1 = chunk[0];
        let y1 = chunk[1];
        let x2 = chunk[2];
        let y2 = chunk[3];
        let cx = (x1 + x2) * 0.5;
        let cy = (y1 + y2) * 0.5;
        let half_w = (x2 - x1).abs() * 0.5 * scale;
        let half_h = (y2 - y1).abs() * 0.5 * scale;
        out.push((cx - half_w).clamp(0.0, 1.0));
        out.push((cy - half_h).clamp(0.0, 1.0));
        out.push((cx + half_w).clamp(0.0, 1.0));
        out.push((cy + half_h).clamp(0.0, 1.0));
    }

    let out_shape = if shape == vec![2, 2] {
        vec![2, 2]
    } else if shape.len() == 3 && shape[1] == 2 && shape[2] == 2 {
        vec![shape[0], 2, 2]
    } else {
        vec![out.len() / 4, 2, 2]
    };

    let py_tensor = Tensor::new(out, out_shape);
    wrap_tensor_dict(py_tensor.to_py_dict(vm, DType::Float32)?.into(), vm)
}

pub fn make_rect_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.geom.rect", vm.ctx.new_dict(), None);
    module
        .set_attr("containing", vm.new_function("containing", containing), vm)
        .unwrap();
    module
        .set_attr("buffer", vm.new_function("buffer", buffer), vm)
        .unwrap();
    module
}
