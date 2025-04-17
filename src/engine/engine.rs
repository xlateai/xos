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

pub struct CursorStyleSetter<'a> {
    style: &'a mut CursorStyle,
}

impl<'a> CursorStyleSetter<'a> {
    fn set(&mut self, val: CursorStyle) {
        *self.style = val;
    }
}

macro_rules! impl_cursor_style_setters {
    ($($variant:ident => $method:ident),* $(,)?) => {
        impl<'a> CursorStyleSetter<'a> {
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

pub struct MouseState<'a> {
    pub x: f32,
    pub y: f32,
    pub is_down: bool,
    pub style: CursorStyleSetter<'a>,
}

pub struct EngineState<'a> {
    pub frame: FrameState,
    pub mouse: MouseState<'a>,
}
