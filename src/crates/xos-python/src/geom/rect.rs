use crate::dtypes::DType;
use crate::tensors::{tensor_flat_data_list, tensor_shape_tuple, Tensor};
use rustpython_vm::{
    builtins::{PyList, PyModule, PyTuple},
    function::FuncArgs,
    PyObjectRef, PyRef, PyResult, VirtualMachine,
};

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

/// Resolve a `(x, y)` point from a Python tuple, list, or 2-element tensor / sequence.
fn extract_point_xy(obj: &PyObjectRef, vm: &VirtualMachine) -> PyResult<(f32, f32)> {
    if let Some(tup) = obj.downcast_ref::<PyTuple>() {
        let slc = tup.as_slice();
        if slc.len() != 2 {
            return Err(vm.new_value_error("point must have exactly 2 elements".to_string()));
        }
        let x = py_number_to_f32(slc[0].clone(), vm, "point.x")?;
        let y = py_number_to_f32(slc[1].clone(), vm, "point.y")?;
        return Ok((x, y));
    }
    if let Some(lst) = obj.downcast_ref::<PyList>() {
        let v = lst.borrow_vec();
        if v.len() != 2 {
            return Err(vm.new_value_error("point must have exactly 2 elements".to_string()));
        }
        let x = py_number_to_f32(v[0].clone(), vm, "point.x")?;
        let y = py_number_to_f32(v[1].clone(), vm, "point.y")?;
        return Ok((x, y));
    }
    let flat = tensor_flat_data_list(obj, vm)?;
    if flat.len() != 2 {
        return Err(vm.new_value_error("point must have exactly 2 elements".to_string()));
    }
    Ok((flat[0], flat[1]))
}

/// `check_point_in_hitboxes(hitboxes, point)` → bool tensor of shape `(N,)`.
///
/// `hitboxes` is `(N, 2, 2)` (or a single `(2, 2)` rect) of axis-aligned `[[x1,y1],[x2,y2]]` rects.
/// `point` is `(x, y)` in the same coordinate space (tuple, list, or length-2 tensor).
fn check_point_in_hitboxes(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() != 2 {
        return Err(vm.new_type_error(format!(
            "check_point_in_hitboxes() takes exactly 2 arguments ({} given)",
            args_vec.len()
        )));
    }

    let flat = tensor_flat_data_list(&args_vec[0], vm)?;
    let shape = tensor_shape_tuple(&args_vec[0], vm).unwrap_or_default();
    let (px, py) = extract_point_xy(&args_vec[1], vm)?;

    let n = if flat.is_empty() {
        0
    } else if shape == vec![2, 2] {
        1
    } else if shape.len() == 3 && shape[1] == 2 && shape[2] == 2 {
        shape[0]
    } else if flat.len() % 4 == 0 {
        flat.len() / 4
    } else {
        return Err(vm.new_type_error(
            "hitboxes must have shape (2,2), (N,2,2), or flat length multiple of 4".to_string(),
        ));
    };

    let mut mask = Vec::with_capacity(n);
    for i in 0..n {
        let base = i * 4;
        let x1 = flat[base];
        let y1 = flat[base + 1];
        let x2 = flat[base + 2];
        let y2 = flat[base + 3];
        let xa = x1.min(x2);
        let xb = x1.max(x2);
        let ya = y1.min(y2);
        let yb = y1.max(y2);
        let inside = px >= xa && px < xb && py >= ya && py < yb;
        mask.push(if inside { 1.0 } else { 0.0 });
    }

    let py_tensor = Tensor::new(mask, vec![n]);
    wrap_tensor_dict(py_tensor.to_py_dict(vm, DType::Bool)?, vm)
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
        .set_attr(
            "check_point_in_hitboxes",
            vm.new_function("check_point_in_hitboxes", check_point_in_hitboxes),
            vm,
        )
        .unwrap();
    module
}
