//! XOS command-line interface and compile helpers.

pub mod apps_cli;
pub mod compile;

#[cfg(all(not(target_arch = "wasm32"), any(target_os = "macos", target_os = "windows")))]
pub mod daemon_remote;

#[cfg(not(target_arch = "wasm32"))]
pub mod daemon;
