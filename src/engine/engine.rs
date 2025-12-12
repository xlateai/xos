use crate::tensor::array::Array;

#[derive(Debug)]
pub struct FrameState {
    /// Array with shape [height, width, 4] for RGBA pixels
    array: Array<u8>,
}

impl FrameState {
    /// Create a new FrameState with the given dimensions
    pub fn new(width: u32, height: u32) -> Self {
        let shape = vec![height as usize, width as usize, 4];
        let data = vec![0u8; (width * height * 4) as usize];
        Self {
            array: Array::new(data, shape),
        }
    }

    /// Get the width of the frame
    pub fn width(&self) -> u32 {
        let shape = self.array.shape();
        shape[1] as u32
    }

    /// Get the height of the frame
    pub fn height(&self) -> u32 {
        let shape = self.array.shape();
        shape[0] as u32
    }

    /// Get mutable access to the buffer (zero-copy for CPU arrays)
    /// Panics if the array is on a non-CPU device
    pub fn buffer_mut(&mut self) -> &mut [u8] {
        self.array.data_mut()
    }

    /// Get the underlying array
    pub fn array(&self) -> &Array<u8> {
        &self.array
    }

    /// Resize the frame to new dimensions
    pub fn resize(&mut self, width: u32, height: u32) {
        let shape = vec![height as usize, width as usize, 4];
        let data = vec![0u8; (width * height * 4) as usize];
        self.array = Array::new(data, shape);
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
    pub frame: FrameState,
    pub mouse: MouseState,
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
