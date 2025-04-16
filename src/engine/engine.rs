#[derive(Debug, Clone)]
pub struct EngineState {
    pub frame: FrameState,
    pub mouse: MouseState,
}

#[derive(Debug, Clone)]
pub struct FrameState {
    pub width: u32,
    pub height: u32,
    pub buffer: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct MouseState {
    pub x: f32,
    pub y: f32,
    pub is_down: bool,
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
