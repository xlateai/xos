//! Mesh helpers: run arbitrary mesh Python scripts (e.g. terminal/status).

mod app;

pub use app::{run_mesh_app, run_mesh_python_file};
pub use xos_mesh::*;
