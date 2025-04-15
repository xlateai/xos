use crate::engine::{Application, EngineState};
use cosmic_text::{Buffer, FontSystem, Metrics, SwashCache, Attrs, Action};
use tiny_skia::PixmapMut;

const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);
const FONT_SIZE: f32 = 16.0;

pub struct TextApp {
    font_system: FontSystem,
    buffer: Buffer,
    cache: SwashCache,
    scroll_y: f32,
}

impl TextApp {
    pub fn new() -> Self {
        let mut font_system = FontSystem::new();
        let metrics = Metrics::new(FONT_SIZE, FONT_SIZE);
        let mut buffer = Buffer::new(&mut font_system, metrics);
        buffer.set_redraw(true);

        Self {
            font_system,
            buffer,
            cache: SwashCache::new(),
            scroll_y: 0.0,
        }
    }
}

impl Application for TextApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let width = state.frame.width;
        let height = state.frame.height;
        let buffer = &mut state.frame.buffer;

        // Clear frame
        for i in (0..buffer.len()).step_by(4) {
            buffer[i + 0] = BACKGROUND_COLOR.0;
            buffer[i + 1] = BACKGROUND_COLOR.1;
            buffer[i + 2] = BACKGROUND_COLOR.2;
            buffer[i + 3] = 0xff;
        }

        // Update buffer size
        self.buffer.set_size(&mut self.font_system, Some(width as f32), Some(height as f32));
        self.buffer.shape_until_scroll(&mut self.font_system, false);

        // Render using a closure
        let stride = width * 4;
        let height_i32 = height as i32;
        self.buffer.draw(&mut self.font_system, &mut self.cache, |x, y, w, h, color| {
            for iy in 0..h {
                for ix in 0..w {
                    let px = x + ix as i32;
                    let py = y + iy as i32;
                    if px >= 0 && py >= 0 && px < width as i32 && py < height_i32 {
                        let idx = (py as u32 * width + px as u32) * 4;
                        let idx = idx as usize;
                        buffer[idx + 0] = color.r();
                        buffer[idx + 1] = color.g();
                        buffer[idx + 2] = color.b();
                        buffer[idx + 3] = color.a();
                    }
                }
            }
        });
    }

    fn on_scroll(&mut self, _state: &mut EngineState, _dx: f32, dy: f32) {
        self.scroll_y -= dy;
        self.scroll_y = self.scroll_y.max(0.0);
    }

    fn on_key_char(&mut self, _state: &mut EngineState, ch: char) {
        if ch == '\u{8}' {
            self.buffer.action(Action::Backspace);
        } else if ch == '\r' || ch == '\n' {
            self.buffer.action(Action::Enter);
        } else if !ch.is_control() {
            self.buffer.action(Action::Insert(ch));
        }
    }
}
