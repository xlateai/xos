use rustpython_vm::{PyResult, VirtualMachine};

mod conv;
mod linear;

pub use conv::{conv2d_forward, conv2d_new, relu_forward};
pub use linear::linear_new;

/// Minimal Rust-side counterpart of Python `xos.nn.Module`.
pub trait Module {
    fn module_name(&self) -> &'static str {
        "Module"
    }
}

/// Rust-side Conv2d trait built on top of Module.
pub trait Conv2d: Module {
    fn out_channels(&self) -> usize;
    fn kernel_size(&self) -> (usize, usize);
    fn stride(&self) -> (usize, usize);

    fn output_shape(&self, h: usize, w: usize) -> (usize, usize) {
        let (kh, kw) = self.kernel_size();
        let (sh, sw) = self.stride();
        let oh = (h - kh) / sh + 1;
        let ow = (w - kw) / sw + 1;
        (oh, ow)
    }

    fn forward_conv2d(&self, src: &[f32], h: usize, w: usize, c: usize) -> Vec<f32>;
}

pub(super) fn wrap_tensor_dict(dict: rustpython_vm::PyObjectRef, vm: &VirtualMachine) -> PyResult {
    if let Ok(wrapper_class) = vm.builtins.get_attr("_TensorWrapper", vm) {
        if let Ok(wrapped) = wrapper_class.call((dict.clone(),), vm) {
            return Ok(wrapped);
        }
    }
    Ok(dict)
}
