use crate::tensor::array::{Array, Device};

/// Safe region boundaries for UI elements
/// Coordinates are normalized (0.0 to 1.0) relative to the frame dimensions
#[derive(Debug, Clone)]
pub struct SafeRegionBoundaries {
    /// Top safe region: (left_x, top_y, right_x, bottom_y)
    pub top_safe_coordinates: (f32, f32, f32, f32),
    /// Bottom safe region: (left_x, top_y, right_x, bottom_y)
    pub bottom_safe_coordinates: (f32, f32, f32, f32),
}

impl SafeRegionBoundaries {
    /// Create safe regions for non-iOS devices (full screen, no restrictions)
    pub fn full_screen() -> Self {
        Self {
            top_safe_coordinates: (0.0, 0.0, 1.0, 0.0),
            bottom_safe_coordinates: (0.0, 1.0, 1.0, 1.0),
        }
    }

    /// Create safe regions for iOS devices (iPhone 16 Pro safe areas)
    /// Top safe area accounts for Dynamic Island (~59pt)
    /// Bottom safe area accounts for home indicator (~34pt)
    /// Assuming typical screen dimensions, these translate to normalized coordinates
    pub fn ios_iphone_16_pro() -> Self {
        // iPhone 16 Pro: 393x852 points
        // Top safe area: ~59pt from top (Dynamic Island)
        // Bottom safe area: ~34pt from bottom (home indicator)
        // Normalized: top ~0.069, bottom ~0.960
        Self {
            top_safe_coordinates: (0.0, 0.069, 1.0, 0.069),
            bottom_safe_coordinates: (0.0, 0.960, 1.0, 1.0),
        }
    }
}

/// Frame state containing the pixel array and safe region information
#[derive(Debug)]
pub struct FrameState {
    /// The pixel array with shape [height, width, 4] for RGBA pixels
    pub array: Array<u8>,
    /// Safe region boundaries for UI elements
    pub safe_region_boundaries: SafeRegionBoundaries,
}

impl FrameState {
    /// Create a new FrameState with given dimensions and safe regions
    pub fn new(width: u32, height: u32, safe_regions: SafeRegionBoundaries) -> Self {
        let shape = vec![height as usize, width as usize, 4];
        let data = vec![0u8; (width * height * 4) as usize];
        Self {
            array: Array::new_on_device(data, shape, Device::Cpu),
            safe_region_boundaries: safe_regions,
        }
    }

    /// Get mutable access to the frame buffer (zero-copy for CPU arrays)
    /// Panics if the array is on a non-CPU device
    pub fn buffer_mut(&mut self) -> &mut [u8] {
        self.array.data_mut()
    }

    /// Get the frame shape [height, width, 4]
    pub fn shape(&self) -> Vec<usize> {
        self.array.shape().to_vec()
    }

    /// Resize the frame to new dimensions (preserves safe regions)
    pub fn resize(&mut self, width: u32, height: u32) {
        let shape = vec![height as usize, width as usize, 4];
        let data = vec![0u8; (width * height * 4) as usize];
        self.array = Array::new_on_device(data, shape, Device::Cpu);
        // Safe regions are normalized, so they don't need to change
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
pub struct EngineState {
    /// Frame state containing pixel array and safe region boundaries
    pub frame: FrameState,
    pub mouse: MouseState,
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
}
