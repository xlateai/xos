// --- Platform-specific audio device and listener implementations ---
#[cfg(target_arch = "wasm32")]
mod wasm_device;
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
mod ios_device;
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
mod native_device;

#[cfg(target_arch = "wasm32")]
mod wasm_listener;
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
mod ios_listener;
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
mod native_listener;

// --- Audio player (output/speaker) implementations ---
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
mod native_player;

// --- Public re-exports ---
#[cfg(target_arch = "wasm32")]
pub use wasm_device::{all as devices, print_all as print_devices};
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
pub use ios_device::{all as devices, print_all as print_devices};
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
pub use native_device::{all as devices, print_all as print_devices};

#[cfg(target_arch = "wasm32")]
pub use wasm_listener::AudioListener;
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
pub use ios_listener::AudioListener;
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
pub use native_listener::AudioListener;

// Audio player re-exports
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
pub use native_player::AudioPlayer;
