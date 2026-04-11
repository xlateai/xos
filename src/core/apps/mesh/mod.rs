//! Mesh: TCP star topology + terminal line editor for CLI UX. Modes `local` / `lan` in
//! [`runtime::MeshSession`]; TLS / auth / `online` are future work.

mod mesh;
#[cfg(not(target_arch = "wasm32"))]
mod lan_crypto;
pub mod runtime;
pub mod state;
pub mod terminal;

pub use mesh::{run_mesh_app, MeshApp};
