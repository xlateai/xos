pub mod ai;
pub mod audio;
pub mod auth;
pub mod burn_train;
pub mod coordinates;
pub mod colors;
pub mod csv_api;
pub mod data;
pub mod dialoguer;
pub mod dtypes;
pub mod engine;
pub mod geom;
pub mod manager;
pub mod math;
pub mod mesh;
pub mod mouse;
pub mod nn;
pub mod ops;
pub mod path;
pub mod python_text;
pub mod random;
pub mod rasterizer;
pub mod regex;
pub mod runtime;
pub mod sensors;
pub mod system;
pub mod tensors;
pub mod terminal;
pub mod ui;
pub mod xos_module;

use rustpython_vm::{builtins::PyModule, PyRef, VirtualMachine};

pub use runtime::{
    parse_script_cli_flags, run_python_app, run_python_file, run_python_interactive,
};

pub fn make_tensors_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.tensors", vm.ctx.new_dict(), None);
    tensors::register_tensors_functions(&module, vm);
    module
}
