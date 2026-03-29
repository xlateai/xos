mod fps_overlay;

pub use fps_overlay::{tick_fps_overlay, FpsOverlay};

use crate::tensor::FrameBuffer;

/// Safe region bounding rectangle for UI elements
/// 
/// Defines the safe rectangular area where content can be displayed without being
/// obscured by system UI elements (e.g., notches, home indicators, etc.).
/// 
/// Coordinates are normalized (0.0 to 1.0) relative to the full viewport dimensions.
/// The coordinate system uses:
/// - x1, x2: horizontal coordinates (left to right, 0.0 to 1.0)
/// - y1, y2: vertical coordinates (top to bottom, 0.0 to 1.0)
/// 
/// The rectangle is defined by two corner points:
/// - (x1, y1): top-left corner (minimum x, minimum y)
/// - (x2, y2): bottom-right corner (maximum x, maximum y)
/// 
/// For a full-screen safe region (no restrictions), use (0.0, 0.0, 1.0, 1.0).
#[derive(Debug, Clone)]
pub struct SafeRegionBoundingRectangle {
    /// Left edge of the safe region (minimum x, typically 0.0)
    pub x1: f32,
    /// Top edge of the safe region (minimum y, typically > 0.0 on devices with notches)
    pub y1: f32,
    /// Right edge of the safe region (maximum x, typically 1.0)
    pub x2: f32,
    /// Bottom edge of the safe region (maximum y, typically < 1.0 on devices with home indicators)
    pub y2: f32,
}

impl SafeRegionBoundingRectangle {
    /// Create safe region for non-iOS devices (full screen, no restrictions)
    /// Returns a rectangle covering the entire viewport: (0.0, 0.0, 1.0, 1.0)
    pub fn full_screen() -> Self {
        Self {
            x1: 0.0,
            y1: 0.0,
            x2: 1.0,
            y2: 1.0,
        }
    }

    /// Create safe region for iOS devices (iPhone 16 Pro safe area)
    /// 
    /// Accounts for:
    /// - Dynamic Island at the top (~59pt, normalized to ~0.069)
    /// - Home indicator at the bottom (~34pt, normalized to ~0.960)
    /// 
    /// Returns a rectangle that excludes these areas.
    /// For iPhone 16 Pro (393x852 points), this is approximately (0.0, 0.069, 1.0, 0.960)
    pub fn ios_iphone_16_pro() -> Self {
        // iPhone 16 Pro: 393x852 points
        // Top safe area: ~59pt from top (Dynamic Island)
        // Bottom safe area: ~34pt from bottom (home indicator)
        // Normalized: top ~0.069, bottom ~0.960
        Self {
            x1: 0.0,
            y1: 0.08,  // Top edge starts below Dynamic Island
            x2: 1.0,
            y2: 0.95,  // Bottom edge ends above home indicator
        }
    }
}

/// Frame state containing the pixel array and safe region information
#[derive(Debug)]
pub struct FrameState {
    /// The pixel buffer with shape [height, width, 4] for RGBA pixels
    pub array: FrameBuffer,
    /// Safe region bounding rectangle for UI elements
    pub safe_region_boundaries: SafeRegionBoundingRectangle,
}

impl FrameState {
    /// Create a new FrameState with given dimensions and safe region
    pub fn new(width: u32, height: u32, safe_region: SafeRegionBoundingRectangle) -> Self {
        Self {
            array: FrameBuffer::new(width, height),
            safe_region_boundaries: safe_region,
        }
    }

    /// Get mutable access to the frame buffer (zero-copy for rasterizer)
    pub fn buffer_mut(&mut self) -> &mut [u8] {
        self.array.buffer_mut()
    }

    /// Get the frame shape [height, width, 4]
    pub fn shape(&self) -> Vec<usize> {
        self.array.shape().to_vec()
    }

    /// Resize the frame to new dimensions (preserves safe region, as it's normalized)
    pub fn resize(&mut self, width: u32, height: u32) {
        self.array.resize(width, height);
    }

    /// Clear the frame buffer to black (all zeros)
    pub fn clear(&mut self) {
        self.array.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    Default,
    Text,
    ResizeHorizontal,
    ResizeVertical,
    ResizeDiagonalNE,
    ResizeDiagonalNW,
    Hand,
    Crosshair,
    Hidden,
}

#[derive(Debug)]
pub struct CursorStyleSetter {
    current_style: CursorStyle,
}

impl CursorStyleSetter {
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            current_style: CursorStyle::Default,
        }
    }

    #[inline(always)]
    pub fn get(&self) -> CursorStyle {
        self.current_style
    }

    #[inline(always)]
    fn set(&mut self, val: CursorStyle) {
        self.current_style = val;
    }
}


macro_rules! impl_cursor_style_setters {
    ($($variant:ident => $method:ident),* $(,)?) => {
        impl CursorStyleSetter {
            $(
                #[inline(always)]
                pub fn $method(&mut self) {
                    self.set(CursorStyle::$variant);
                }
            )*
        }
    };
}

impl_cursor_style_setters! {
    Default => default,
    Text => text,
    ResizeHorizontal => resize_horizontal,
    ResizeVertical => resize_vertical,
    ResizeDiagonalNE => resize_diagonal_ne,
    ResizeDiagonalNW => resize_diagonal_nw,
    Hand => hand,
    Crosshair => crosshair,
    Hidden => hidden,
}

#[derive(Debug)]
pub struct MouseState {
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
    pub is_left_clicking: bool,
    pub is_right_clicking: bool,
    pub style: CursorStyleSetter,
}

#[derive(Debug)]
pub struct KeyboardState {
    pub onscreen: crate::text::onscreen_keyboard::OnScreenKeyboard,
}

#[derive(Debug)]
pub struct EngineState {
    /// Frame state containing pixel array and safe region boundaries
    pub frame: FrameState,
    pub mouse: MouseState,
    pub keyboard: KeyboardState,
    /// Global FPS overlay (drawn by the engine after each app tick).
    pub fps_overlay: FpsOverlay,
}

impl EngineState {
    /// Get mutable access to the frame buffer (zero-copy for CPU arrays)
    /// Panics if the array is on a non-CPU device
    pub fn frame_buffer_mut(&mut self) -> &mut [u8] {
        self.frame.buffer_mut()
    }

    /// Resize the frame to new dimensions
    pub fn resize_frame(&mut self, width: u32, height: u32) {
        self.frame.resize(width, height);
    }
}

pub trait Application {
    fn setup(&mut self, state: &mut EngineState) -> Result<(), String>;
    fn tick(&mut self, state: &mut EngineState);

    fn on_mouse_down(&mut self, _state: &mut EngineState) {}
    fn on_mouse_up(&mut self, _state: &mut EngineState) {}
    fn on_mouse_move(&mut self, _state: &mut EngineState) {}
    fn on_scroll(&mut self, _state: &mut EngineState, _delta_x: f32, _delta_y: f32) {}
    fn on_key_char(&mut self, _state: &mut EngineState, _ch: char) {}
    fn on_key_shortcut(&mut self, _state: &mut EngineState, _shortcut: crate::keyboard::shortcuts::ShortcutAction) {}
    fn on_screen_size_change(&mut self, _state: &mut EngineState, _width: u32, _height: u32) {}
}
