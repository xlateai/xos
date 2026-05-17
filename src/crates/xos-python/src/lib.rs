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
pub(crate) mod json_codec;
pub mod json_api;
pub mod manager;
pub mod math;
pub mod mesh;
pub mod mouse;
pub mod nn;
pub mod ops;
pub mod path;
pub mod python_whiteboard;
pub mod whiteboard_kernel;
pub mod random;
pub mod rasterizer;
pub mod regex;
pub mod runtime;
pub(crate) mod staged_native_python_app;
pub mod sensors;
pub mod system;
pub mod tensor_buf;
pub mod tensors;
pub mod terminal;
pub mod ui_events;
pub mod xos_module;

use rustpython_vm::{builtins::PyModule, PyRef, VirtualMachine};

pub use json_codec::decode_mesh_jpeg_bytes_best_effort;
pub use runtime::{
    parse_script_cli_flags, run_python_app, run_python_file, run_python_interactive,
};

pub fn make_tensors_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.tensors", vm.ctx.new_dict(), None);
    tensors::register_tensors_functions(&module, vm);
    module
}
