pub mod runtime;
pub mod xos_module;
pub mod random;
pub mod engine;
pub mod rasterizer;
pub mod tensors;
pub mod sensors;
pub mod audio;
pub mod system;
pub mod dialoguer;
pub mod math;
pub mod ops;
pub mod colors;
pub mod dtypes;
pub mod data;
pub mod ui;
pub mod nn;
pub mod burn_train;
pub mod mesh;
pub mod mouse;

use rustpython_vm::{PyRef, VirtualMachine, builtins::PyModule};

pub use runtime::{run_python_file, run_python_interactive, run_python_app};

pub fn make_tensors_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.tensors", vm.ctx.new_dict(), None);
    tensors::register_tensors_functions(&module, vm);
    module
}
