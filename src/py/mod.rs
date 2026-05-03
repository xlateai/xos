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
pub mod path;
pub mod ui;
pub mod python_text;
pub mod nn;
pub mod burn_train;
pub mod mesh;
pub mod mouse;
pub mod terminal;
pub mod manager;
pub mod auth;
pub mod ai;

use rustpython_vm::{PyRef, VirtualMachine, builtins::PyModule};

pub use runtime::{parse_script_cli_flags, run_python_app, run_python_file, run_python_interactive};

pub fn make_tensors_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.tensors", vm.ctx.new_dict(), None);
    tensors::register_tensors_functions(&module, vm);
    module
}
