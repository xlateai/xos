//! Burn-backed autograd, MSE loss, and Adam for `xos.nn` (Autodiff + ndarray CPU backend).

use burn::nn::LinearConfig;
use burn::optim::adaptor::OptimizerAdaptor;
use burn::optim::{AdamConfig, GradientsParams, Optimizer};
use burn::tensor::backend::AutodiffBackend;
use burn_autodiff::Autodiff;
use burn::tensor::{Tensor, TensorData};
use burn_ndarray::NdArray;
use once_cell::sync::Lazy;
use rustpython_vm::{
    PyObjectRef, PyRef, PyResult, VirtualMachine, builtins::PyList, builtins::PyModule, function::FuncArgs,
};
use std::collections::HashMap;
use std::sync::Mutex;

/// Autodiff over ndarray — fast for small batches without GPU init/sync.
type TrainAD = Autodiff<NdArray<f32>>;

struct BurnRuntime {
    next_id: u64,
    linears: HashMap<u64, burn::nn::Linear<TrainAD>>,
    optimizers: HashMap<u64, OptimizerAdaptor<burn::optim::Adam, burn::nn::Linear<TrainAD>, TrainAD>>,
    optim_linear: HashMap<u64, u64>,
    last_pred: HashMap<u64, Tensor<TrainAD, 2>>,
    last_loss: Option<Tensor<TrainAD, 1>>,
    last_grads: Option<<TrainAD as AutodiffBackend>::Gradients>,
    last_loss_scalar: f64,
    last_loss_linear_id: Option<u64>,
}

impl Default for BurnRuntime {
    fn default() -> Self {
        Self {
            next_id: 1,
            linears: HashMap::new(),
            optimizers: HashMap::new(),
            optim_linear: HashMap::new(),
            last_pred: HashMap::new(),
            last_loss: None,
            last_grads: None,
            last_loss_scalar: 0.0,
            last_loss_linear_id: None,
        }
    }
}

static RUNTIME: Lazy<Mutex<BurnRuntime>> = Lazy::new(|| Mutex::new(BurnRuntime::default()));

fn device() -> <NdArray<f32> as burn::tensor::backend::Backend>::Device {
    Default::default()
}

fn wrap_tensor_dict(dict: PyObjectRef, vm: &VirtualMachine) -> PyResult {
    if let Ok(wrapper_class) = vm.builtins.get_attr("_TensorWrapper", vm) {
        if let Ok(wrapped) = wrapper_class.call((dict.clone(),), vm) {
            return Ok(wrapped);
        }
    }
    Ok(dict)
}

fn next_id(rt: &mut BurnRuntime) -> u64 {
    let id = rt.next_id;
    rt.next_id += 1;
    id
}

/// `_burn.linear_register(in_features, out_features, bias=True) -> id`
pub fn linear_register(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let d_in: i32 = args
        .kwargs
        .get("in_features")
        .or_else(|| args.args.get(0))
        .ok_or_else(|| vm.new_type_error("linear_register: in_features required".to_string()))?
        .clone()
        .try_into_value(vm)?;
    let d_out: i32 = args
        .kwargs
        .get("out_features")
        .or_else(|| args.args.get(1))
        .ok_or_else(|| vm.new_type_error("linear_register: out_features required".to_string()))?
        .clone()
        .try_into_value(vm)?;
    let bias: bool = args
        .kwargs
        .get("bias")
        .or_else(|| args.args.get(2))
        .map(|v| v.clone().try_into_value::<bool>(vm).unwrap_or(true))
        .unwrap_or(true);

    let d_in = d_in.max(1) as usize;
    let d_out = d_out.max(1) as usize;

    let mut rt = RUNTIME.lock().map_err(|_| vm.new_runtime_error("burn runtime lock".to_string()))?;
    let dev = device();
    let config = LinearConfig::new(d_in, d_out).with_bias(bias);
    let linear = config.init::<TrainAD>(&dev);
    let id = next_id(&mut rt);
    rt.linears.insert(id, linear);
    Ok(vm.ctx.new_int(id as i64).into())
}

/// `_burn.linear_forward(id, flat_input) -> Tensor`
pub fn linear_forward(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() < 2 {
        return Err(vm.new_type_error("linear_forward(id, flat_input)".to_string()));
    }
    let id: i64 = args_vec[0].clone().try_into_value(vm)?;
    let list = args_vec[1]
        .downcast_ref::<PyList>()
        .ok_or_else(|| vm.new_type_error("flat_input must be a list".to_string()))?;

    let mut flat: Vec<f32> = Vec::new();
    fn flatten(obj: &PyObjectRef, out: &mut Vec<f32>, vm: &VirtualMachine) -> PyResult<()> {
        if let Some(l) = obj.downcast_ref::<PyList>() {
            for x in l.borrow_vec().iter() {
                flatten(x, out, vm)?;
            }
        } else {
            use crate::tensor::tensor::py_number_to_f64;
            out.push(py_number_to_f64(obj, vm)? as f32);
        }
        Ok(())
    }
    for x in list.borrow_vec().iter() {
        flatten(x, &mut flat, vm)?;
    }

    let mut rt = RUNTIME.lock().map_err(|_| vm.new_runtime_error("burn runtime lock".to_string()))?;
    let d_in = rt
        .linears
        .get(&(id as u64))
        .ok_or_else(|| vm.new_value_error("unknown linear id".to_string()))?
        .weight
        .val()
        .shape()
        .dims[0];

    if flat.len() < d_in {
        flat.resize(d_in, 0.0);
    } else if flat.len() > d_in {
        flat.truncate(d_in);
    }

    let dev = device();
    let input: Tensor<TrainAD, 2> = Tensor::from_data(TensorData::new(flat, [1, d_in]), &dev);
    let linear = rt.linears.get(&(id as u64)).unwrap();
    let out: Tensor<TrainAD, 2> = linear.forward(input);
    rt.last_pred.insert(id as u64, out.clone());

    let out_data = out.clone().into_data();
    let shape_vec = out_data.shape.clone();
    let flat_out: Vec<f32> = out_data
        .to_vec::<f32>()
        .map_err(|e| vm.new_runtime_error(format!("tensor to_vec: {e:?}")))?;

    let dict = vm.ctx.new_dict();
    dict.set_item(
        "shape",
        vm.ctx
            .new_tuple(shape_vec.iter().map(|&s| vm.ctx.new_int(s as i64).into()).collect())
            .into(),
        vm,
    )?;
    dict.set_item("dtype", vm.ctx.new_str("float32").into(), vm)?;
    dict.set_item("device", vm.ctx.new_str("cpu").into(), vm)?;
    let py_data: Vec<PyObjectRef> = flat_out.iter().map(|&f| vm.ctx.new_float(f as f64).into()).collect();
    dict.set_item("_data", vm.ctx.new_list(py_data).into(), vm)?;
    dict.set_item(
        "_rust_tensor",
        vm.ctx.new_int(0i64).into(),
        vm,
    )?;

    wrap_tensor_dict(dict.into(), vm)
}

/// `_burn.mse_loss(linear_id, target_flat) -> None` (stores loss); use item/backward
pub fn mse_loss(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() < 2 {
        return Err(vm.new_type_error("mse_loss(linear_id, target_flat)".to_string()));
    }
    let linear_id: i64 = args_vec[0].clone().try_into_value(vm)?;
    let list = args_vec[1]
        .downcast_ref::<PyList>()
        .ok_or_else(|| vm.new_type_error("target must be a list".to_string()))?;

    let mut target_flat: Vec<f32> = Vec::new();
    fn flatten(obj: &PyObjectRef, out: &mut Vec<f32>, vm: &VirtualMachine) -> PyResult<()> {
        if let Some(l) = obj.downcast_ref::<PyList>() {
            for x in l.borrow_vec().iter() {
                flatten(x, out, vm)?;
            }
        } else {
            use crate::tensor::tensor::py_number_to_f64;
            out.push(py_number_to_f64(obj, vm)? as f32);
        }
        Ok(())
    }
    for x in list.borrow_vec().iter() {
        flatten(x, &mut target_flat, vm)?;
    }

    let mut rt = RUNTIME.lock().map_err(|_| vm.new_runtime_error("burn runtime lock".to_string()))?;
    let pred = rt
        .last_pred
        .get(&(linear_id as u64))
        .ok_or_else(|| vm.new_value_error("run linear_forward before mse_loss".to_string()))?
        .clone();

    let n = pred.shape().num_elements();
    if target_flat.len() < n {
        target_flat.resize(n, 0.0);
    } else {
        target_flat.truncate(n);
    }

    let dev = device();
    let target: Tensor<TrainAD, 2> = Tensor::from_data(TensorData::new(target_flat, pred.shape().dims), &dev);
    let diff = pred - target;
    let loss = diff.powi_scalar(2).mean();
    let loss_scalar = loss.clone().into_scalar();

    rt.last_loss = Some(loss);
    rt.last_grads = None;
    rt.last_loss_scalar = loss_scalar as f64;
    rt.last_loss_linear_id = Some(linear_id as u64);
    Ok(vm.ctx.none())
}

pub fn loss_item(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let rt = RUNTIME.lock().map_err(|_| vm.new_runtime_error("burn runtime lock".to_string()))?;
    Ok(vm.ctx.new_float(rt.last_loss_scalar).into())
}

pub fn loss_backward(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let mut rt = RUNTIME.lock().map_err(|_| vm.new_runtime_error("burn runtime lock".to_string()))?;
    let loss = rt
        .last_loss
        .take()
        .ok_or_else(|| vm.new_runtime_error("no loss; call mse_loss first".to_string()))?;
    let grads = loss.backward();
    rt.last_grads = Some(grads);
    Ok(vm.ctx.none())
}

/// `_burn.adam_init(linear_id, lr, beta1, beta2, eps) -> optim_id`
pub fn adam_init(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let linear_id: i64 = args
        .kwargs
        .get("linear_id")
        .or_else(|| args.args.get(0))
        .ok_or_else(|| vm.new_type_error("adam_init: linear_id".to_string()))?
        .clone()
        .try_into_value(vm)?;
    let lr: f64 = args
        .kwargs
        .get("lr")
        .or_else(|| args.args.get(1))
        .map(|v| v.clone().try_into_value::<f64>(vm).unwrap_or(0.001))
        .unwrap_or(0.001);
    let b1: f32 = args
        .kwargs
        .get("beta1")
        .or_else(|| args.args.get(2))
        .map(|v| v.clone().try_into_value::<f64>(vm).unwrap_or(0.9) as f32)
        .unwrap_or(0.9);
    let b2: f32 = args
        .kwargs
        .get("beta2")
        .or_else(|| args.args.get(3))
        .map(|v| v.clone().try_into_value::<f64>(vm).unwrap_or(0.999) as f32)
        .unwrap_or(0.999);
    let eps: f32 = args
        .kwargs
        .get("eps")
        .or_else(|| args.args.get(4))
        .map(|v| v.clone().try_into_value::<f64>(vm).unwrap_or(1e-8) as f32)
        .unwrap_or(1e-8);

    let mut rt = RUNTIME.lock().map_err(|_| vm.new_runtime_error("burn runtime lock".to_string()))?;
    if !rt.linears.contains_key(&(linear_id as u64)) {
        return Err(vm.new_value_error("unknown linear_id".to_string()));
    }

    let config = AdamConfig::new()
        .with_beta_1(b1)
        .with_beta_2(b2)
        .with_epsilon(eps);
    let optim: OptimizerAdaptor<burn::optim::Adam, burn::nn::Linear<TrainAD>, TrainAD> = config.init();

    let oid = next_id(&mut rt);
    rt.optimizers.insert(oid, optim);
    rt.optim_linear.insert(oid, linear_id as u64);
    let _ = lr; // stored on step
    Ok(vm.ctx.new_int(oid as i64).into())
}

/// `_burn.adam_step(optim_id, lr)`
pub fn adam_step(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.is_empty() {
        return Err(vm.new_type_error("adam_step(optim_id, lr)".to_string()));
    }
    let optim_id: i64 = args_vec[0].clone().try_into_value(vm)?;
    let lr: f64 = args_vec
        .get(1)
        .map(|v| v.clone().try_into_value::<f64>(vm).unwrap_or(0.001))
        .unwrap_or(0.001);

    let mut rt = RUNTIME.lock().map_err(|_| vm.new_runtime_error("burn runtime lock".to_string()))?;
    let linear_id = *rt
        .optim_linear
        .get(&(optim_id as u64))
        .ok_or_else(|| vm.new_value_error("unknown optim_id".to_string()))?;
    let grads = rt
        .last_grads
        .take()
        .ok_or_else(|| vm.new_runtime_error("call loss_backward before adam_step".to_string()))?;

    let linear = rt
        .linears
        .remove(&linear_id)
        .ok_or_else(|| vm.new_runtime_error("linear missing".to_string()))?;
    let optim = rt
        .optimizers
        .get_mut(&(optim_id as u64))
        .ok_or_else(|| vm.new_runtime_error("optimizer missing".to_string()))?;

    let gp = GradientsParams::from_grads(grads, &linear);
    let new_linear = optim.step(lr, linear, gp);
    rt.linears.insert(linear_id, new_linear);
    Ok(vm.ctx.none())
}

pub fn adam_zero_grad(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let mut rt = RUNTIME.lock().map_err(|_| vm.new_runtime_error("burn runtime lock".to_string()))?;
    rt.last_grads = None;
    rt.last_loss = None;
    Ok(vm.ctx.none())
}

pub fn register_burn_module(parent: &PyRef<PyModule>, vm: &VirtualMachine) {
    let burn = vm.new_module("_burn", vm.ctx.new_dict(), None);
    burn
        .set_attr("linear_register", vm.new_function("linear_register", linear_register), vm)
        .unwrap();
    burn
        .set_attr("linear_forward", vm.new_function("linear_forward", linear_forward), vm)
        .unwrap();
    burn.set_attr("mse_loss", vm.new_function("mse_loss", mse_loss), vm).unwrap();
    burn.set_attr("loss_item", vm.new_function("loss_item", loss_item), vm).unwrap();
    burn
        .set_attr("loss_backward", vm.new_function("loss_backward", loss_backward), vm)
        .unwrap();
    burn.set_attr("adam_init", vm.new_function("adam_init", adam_init), vm).unwrap();
    burn.set_attr("adam_step", vm.new_function("adam_step", adam_step), vm).unwrap();
    burn
        .set_attr("adam_zero_grad", vm.new_function("adam_zero_grad", adam_zero_grad), vm)
        .unwrap();
    parent.set_attr("_burn", burn, vm).unwrap();
}
