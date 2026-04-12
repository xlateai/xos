//! Mesh app (`xos app mesh`): [`MeshApp`] + [`run_mesh_app`], and `mesh.py` chat demo.
//! Transport lives in [`crate::mesh`].

mod app;

pub use app::{run_mesh_app, run_mesh_python_file, MeshApp};
pub use crate::mesh::*;
