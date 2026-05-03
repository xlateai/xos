mod f3_menu;

pub mod engine;

pub mod audio;
pub mod keyboard;
pub mod sensors;
pub mod viewport_double_tap;

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
    f3_menu_boost_interaction_fade,
    f3_menu_handle_frame_zoom_scroll,
    f3_menu_handle_mouse_down, f3_menu_handle_mouse_move, f3_menu_handle_mouse_up,
    f3_menu_handle_zoom_scroll, tick_f3_menu, tick_overlay_red_pointer,
    tick_overlay_red_pointer_xy,
    F3Menu,
};
#[cfg(target_os = "ios")]
pub use f3_menu::IosRemoteMeshTransport;
pub use viewport_double_tap::ViewportDoubleTap;
pub use engine::{
    apply_frame_view_zoom, f3_ui_scale_multiplier, frame_view_pan_by_pixels,
    frame_view_rect_norm, tick_frame_delta, tick_frame_view_zoom, Application,
    CursorStyleSetter, EngineState,
    F3_UI_SCALE_DEFAULT_PERCENT, F3_UI_SCALE_MAX_PERCENT, F3_UI_SCALE_MIN_PERCENT,
    FRAME_VIEW_ZOOM_MAX, FRAME_VIEW_ZOOM_MIN, FrameState, KeyboardState, MouseState,
    SafeRegionBoundingRectangle,
};
