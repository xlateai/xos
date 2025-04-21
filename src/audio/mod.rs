// --- Platform-specific audio device and listener implementations ---
#[cfg(target_arch = "wasm32")]
mod wasm_device;
#[cfg(not(target_arch = "wasm32"))]
mod native_device;

#[cfg(target_arch = "wasm32")]
mod wasm_listener;
#[cfg(not(target_arch = "wasm32"))]
mod native_listener;

// --- Public re-exports ---
#[cfg(target_arch = "wasm32")]
pub use wasm_device::{all as devices, print_all as print_devices};
#[cfg(not(target_arch = "wasm32"))]
pub use native_device::{all as devices, print_all as print_devices};

#[cfg(target_arch = "wasm32")]
pub use wasm_listener::AudioListener;
#[cfg(not(target_arch = "wasm32"))]
pub use native_listener::AudioListener;
