use crate::rasterizer::fill_rect_buffer;

pub struct Button {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub text: String,
    pub bg_color: (u8, u8, u8),
    pub hover_color: (u8, u8, u8),
    pub text_color: (u8, u8, u8),
}

impl Button {
    pub fn new(x: i32, y: i32, width: u32, height: u32, text: String) -> Self {
        Self {
            x,
            y,
            width,
            height,
            text,
            bg_color: (50, 150, 50),
            hover_color: (70, 170, 70),
            text_color: (255, 255, 255),
        }
    }

    pub fn draw(&self, buffer: &mut [u8], canvas_width: u32, canvas_height: u32, is_hovered: bool) {
        let color = if is_hovered {
            self.hover_color
        } else {
            self.bg_color
        };
        let cw = canvas_width as usize;
        let ch = canvas_height as usize;
        let x = self.x;
        let y = self.y;
        let x1 = x + self.width as i32;
        let y1 = y + self.height as i32;
        fill_rect_buffer(
            buffer,
            cw,
            ch,
            x,
            y,
            x1,
            y1,
            (color.0, color.1, color.2, 0xff),
        );
        let w = (255, 255, 255, 0xff);
        fill_rect_buffer(buffer, cw, ch, x, y, x1, y + 1, w);
        fill_rect_buffer(buffer, cw, ch, x, y1 - 1, x1, y1, w);
        fill_rect_buffer(buffer, cw, ch, x, y, x + 1, y1, w);
        fill_rect_buffer(buffer, cw, ch, x1 - 1, y, x1, y1, w);
    }

    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        x >= self.x as f32
            && x < (self.x + self.width as i32) as f32
            && y >= self.y as f32
            && y < (self.y + self.height as i32) as f32
    }
}
