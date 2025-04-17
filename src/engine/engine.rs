#[derive(Debug, Clone)]
pub struct FrameState {
    pub width: u32,
    pub height: u32,
    pub buffer: Vec<u8>,
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
}

#[derive(Debug)]
pub struct MouseState {
    pub x: f32,
    pub y: f32,
    pub is_down: bool,
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
