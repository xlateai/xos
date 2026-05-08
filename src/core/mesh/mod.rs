//! TCP mesh: star topology (coordinator + peers), `local` / `lan` modes.
//!
//! Layout: [`mesh`] session module holds transport + [`MeshSession`]; [`nodes`] / [`wire`] /
//! [`graph`] are shared types. The `xos app mesh` runner lives in [`crate::apps::mesh`].

pub mod graph;
mod lan;
#[cfg(not(target_arch = "wasm32"))]
mod lan_crypto;
mod local;
pub mod mesh;
pub mod nodes;
pub mod relay;
pub mod state;
pub mod terminal;
pub mod wire;

pub use mesh::{Inbox, MeshMode, MeshSession, Packet};
