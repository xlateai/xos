mod f3_menu;

pub mod engine;

pub mod audio;
pub mod keyboard;
pub mod sensors;

#[cfg(not(target_arch = "wasm32"))]
pub mod native_engine;

#[cfg(target_arch = "wasm32")]
pub mod wasm_engine;

#[cfg(target_os = "ios")]
pub mod ios_ffi;

// py_engine is now defined in lib.rs as an inline module

#[cfg(not(target_arch = "wasm32"))]
pub use native_engine::start_native;

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
pub use native_engine::{start_overlay_native, NativeLaunchMode};

#[cfg(not(target_arch = "wasm32"))]
pub use native_engine::start_headless_native;

#[cfg(target_arch = "wasm32")]
pub use wasm_engine::run_web;

pub use crate::py_engine::PyApplicationWrapper;

pub use f3_menu::{
    f3_menu_handle_mouse_down, f3_menu_handle_mouse_move, f3_menu_handle_mouse_up, tick_f3_menu,
    F3Menu,
};
pub use engine::{
    tick_frame_delta, Application, CursorStyleSetter, EngineState, FrameState, KeyboardState,
    MouseState, SafeRegionBoundingRectangle,
};
