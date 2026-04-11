//! TCP mesh: star topology (coordinator + peers), `local` / `lan` modes.
//!
//! Layout: [`session`] holds transport + [`MeshSession`]; [`nodes`] / [`wire`] / [`graph`] are
//! shared types for roster + wire format; [`app`] is `xos app mesh`.

mod app;
#[cfg(not(target_arch = "wasm32"))]
mod lan_crypto;
mod lan;
mod local;
pub mod graph;
pub mod mesh;
pub mod nodes;
pub mod relay;
pub mod state;
pub mod terminal;
pub mod wire;

pub use app::{run_mesh_app, MeshApp};
pub use mesh::{Inbox, MeshMode, MeshSession, Packet};
