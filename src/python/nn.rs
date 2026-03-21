//! xos.nn - Neural network modules (Burn-backed)

use rustpython_vm::{
    PyObjectRef, PyResult, VirtualMachine, builtins::{PyDict, PyList, PyModule, PyTuple},
    PyRef, function::FuncArgs,
};
use crate::tensor::{linear_init, linear_forward};

/// Extract flat f32 from array/tensor
fn extract_flat_f32(obj: &PyObjectRef, vm: &VirtualMachine) -> PyResult<Vec<f32>> {
    let list_obj = if let Ok(data_attr) = obj.get_attr("_data", vm) {
        if let Ok(inner_dict) = data_attr.clone().downcast::<PyDict>() {
            inner_dict.get_item("_data", vm).map_err(|e| e)?
        } else if data_attr.downcast_ref::<PyList>().is_some() {
            data_attr
        } else {
            return Err(vm.new_type_error("Tensor _data must be a list".to_string()));
        }
    } else if let Ok(dict) = obj.clone().downcast::<PyDict>() {
        dict.get_item("_data", vm).map_err(|e| e)?
    } else if obj.downcast_ref::<PyList>().is_some() {
        obj.clone()
    } else {
        return Err(vm.new_type_error("Input must be a tensor or list".to_string()));
    };

    let list = list_obj
        .downcast::<PyList>()
        .map_err(|_| vm.new_type_error("_data must be a list".to_string()))?;

    let mut flat = Vec::new();
    fn flatten(item: &PyObjectRef, out: &mut Vec<f32>, vm: &VirtualMachine) -> PyResult<()> {
        if let Some(inner) = item.downcast_ref::<PyList>() {
            for x in inner.borrow_vec().iter() {
                flatten(x, out, vm)?;
            }
        } else {
            let v: f64 = item.clone().try_into_value(vm)
                .or_else(|_| item.clone().try_into_value::<i64>(vm).map(|i| i as f64))?;
            out.push(v as f32);
        }
        Ok(())
    }
    for item in list.borrow_vec().iter() {
        flatten(item, &mut flat, vm)?;
    }
    Ok(flat)
}

/// xos.nn._linear_init(in_features, out_features) -> (weight, bias)
/// Internal: creates initialized weight and bias for Linear layer.
fn linear_init_fn(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let (in_features, out_features): (usize, usize) = args.bind(vm)?;
    let (weight, bias) = linear_init(in_features, out_features);
    let weight_py: Vec<PyObjectRef> = weight.iter().map(|&f| vm.ctx.new_float(f as f64).into()).collect();
    let bias_py: Vec<PyObjectRef> = bias.iter().map(|&f| vm.ctx.new_float(f as f64).into()).collect();
    Ok(vm.ctx.new_tuple(vec![
        vm.ctx.new_list(weight_py).into(),
        vm.ctx.new_list(bias_py).into(),
    ]).into())
}

/// xos.nn._linear_forward(weight, bias, input, in_features, out_features) -> output_tensor
/// Internal: Burn-backed forward pass.
fn linear_forward_fn(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() < 5 {
        return Err(vm.new_type_error(
            "_linear_forward(weight, bias, input, in_features, out_features) requires 5 args".to_string(),
        ));
    }

    let weight = extract_flat_f32(&args_vec[0], vm)?;
    let bias = extract_flat_f32(&args_vec[1], vm)?;
    let input = extract_flat_f32(&args_vec[2], vm)?;
    let in_features: usize = args_vec[3].clone().try_into_value(vm)
        .or_else(|_| args_vec[3].clone().try_into_value::<i64>(vm).map(|i| i as usize))?;
    let out_features: usize = args_vec[4].clone().try_into_value(vm)
        .or_else(|_| args_vec[4].clone().try_into_value::<i64>(vm).map(|i| i as usize))?;

    let batch = input.len() / in_features;
    if batch * in_features != input.len() {
        return Err(vm.new_value_error(
            format!("input size {} not divisible by in_features {}", input.len(), in_features),
        ));
    }

    let output = linear_forward(&weight, &bias, &input, in_features, out_features, batch);

    // Return as xos tensor dict (shape [batch, out_features])
    let dict = vm.ctx.new_dict();
    let py_data: Vec<PyObjectRef> = output.iter().map(|&f| vm.ctx.new_float(f as f64).into()).collect();
    dict.set_item("_data", vm.ctx.new_list(py_data).into(), vm)?;
    dict.set_item(
        "shape",
        vm.ctx.new_tuple(vec![
            vm.ctx.new_int(batch).into(),
            vm.ctx.new_int(out_features).into(),
        ]).into(),
        vm,
    )?;
    dict.set_item("dtype", vm.ctx.new_str("float32").into(), vm)?;
    dict.set_item("device", vm.ctx.new_str("cpu").into(), vm)?;

    // Wrap in _ArrayWrapper if available
    if let Ok(wrapper_class) = vm.builtins.get_attr("_ArrayWrapper", vm) {
        if let Ok(wrapped) = wrapper_class.call((dict.clone(),), vm) {
            return Ok(wrapped);
        }
    }
    Ok(dict.into())
}

const LINEAR_CLASS_CODE: &str = r#"
class Linear:
    """Linear layer: y = x @ weight + bias. Forward pass uses Burn."""
    def __init__(self, in_features, out_features):
        weight, bias = _linear_init(in_features, out_features)
        self._weight = weight
        self._bias = bias
        self.in_features = in_features
        self.out_features = out_features

    def forward(self, x):
        return _linear_forward(self._weight, self._bias, x, self.in_features, self.out_features)

    def __call__(self, x):
        return self.forward(x)

    @property
    def weight(self):
        """Weight matrix (flat, shape in_features * out_features). For 1x1: use weight[0]."""
        return self._weight

    @property
    def bias(self):
        """Bias vector (length out_features). For 1 output: use bias[0]."""
        return self._bias
"#;

pub fn make_nn_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.nn", vm.ctx.new_dict(), None);

    module.set_attr("_linear_init", vm.new_function("_linear_init", linear_init_fn), vm).unwrap();
    module.set_attr("_linear_forward", vm.new_function("_linear_forward", linear_forward_fn), vm).unwrap();

    // Define Linear class
    let scope = vm.new_scope_with_builtins();
    scope.globals.set_item("_linear_init", module.get_attr("_linear_init", vm).unwrap(), vm).unwrap();
    scope.globals.set_item("_linear_forward", module.get_attr("_linear_forward", vm).unwrap(), vm).unwrap();
    if let Err(e) = vm.run_code_string(scope.clone(), LINEAR_CLASS_CODE, "<xos.nn>".to_string()) {
        eprintln!("Failed to create Linear class: {:?}", e);
    }
    if let Ok(linear_class) = scope.globals.get_item("Linear", vm) {
        module.set_attr("Linear", linear_class, vm).unwrap();
    }

    module
}
