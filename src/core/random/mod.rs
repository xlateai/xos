#[cfg(target_arch = "wasm32")]
mod wasm_random;
#[cfg(not(target_arch = "wasm32"))]
mod native_random;

#[cfg(target_arch = "wasm32")]
pub use wasm_random::*;
#[cfg(not(target_arch = "wasm32"))]
pub use native_random::*;
