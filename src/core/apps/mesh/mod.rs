//! Same-machine mesh prototype: TCP on localhost + terminal line editor for clean CLI UX.
//!
//! LAN / QUIC / WebRTC / auth can plug in behind the same session + inbox abstraction later.

mod mesh;
pub mod runtime;
pub mod state;
pub mod terminal;

pub use mesh::{run_mesh_app, MeshApp};
