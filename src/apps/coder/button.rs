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
        let color = if is_hovered { self.hover_color } else { self.bg_color };
        
        // Draw button background
        for dy in 0..self.height {
            for dx in 0..self.width {
                let x = self.x + dx as i32;
                let y = self.y + dy as i32;
                
                if x >= 0 && x < canvas_width as i32 && y >= 0 && y < canvas_height as i32 {
                    let idx = ((y as u32 * canvas_width + x as u32) * 4) as usize;
                    buffer[idx + 0] = color.0;
                    buffer[idx + 1] = color.1;
                    buffer[idx + 2] = color.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }
        
        // Draw button border
        for dx in 0..self.width {
            let x = self.x + dx as i32;
            // Top border
            if self.y >= 0 && self.y < canvas_height as i32 && x >= 0 && x < canvas_width as i32 {
                let idx = ((self.y as u32 * canvas_width + x as u32) * 4) as usize;
                buffer[idx + 0] = 255;
                buffer[idx + 1] = 255;
                buffer[idx + 2] = 255;
                buffer[idx + 3] = 0xff;
            }
            // Bottom border
            let bottom_y = self.y + self.height as i32 - 1;
            if bottom_y >= 0 && bottom_y < canvas_height as i32 && x >= 0 && x < canvas_width as i32 {
                let idx = ((bottom_y as u32 * canvas_width + x as u32) * 4) as usize;
                buffer[idx + 0] = 255;
                buffer[idx + 1] = 255;
                buffer[idx + 2] = 255;
                buffer[idx + 3] = 0xff;
            }
        }
        for dy in 0..self.height {
            let y = self.y + dy as i32;
            // Left border
            if y >= 0 && y < canvas_height as i32 && self.x >= 0 && self.x < canvas_width as i32 {
                let idx = ((y as u32 * canvas_width + self.x as u32) * 4) as usize;
                buffer[idx + 0] = 255;
                buffer[idx + 1] = 255;
                buffer[idx + 2] = 255;
                buffer[idx + 3] = 0xff;
            }
            // Right border
            let right_x = self.x + self.width as i32 - 1;
            if y >= 0 && y < canvas_height as i32 && right_x >= 0 && right_x < canvas_width as i32 {
                let idx = ((y as u32 * canvas_width + right_x as u32) * 4) as usize;
                buffer[idx + 0] = 255;
                buffer[idx + 1] = 255;
                buffer[idx + 2] = 255;
                buffer[idx + 3] = 0xff;
            }
        }
        
        // TODO: Draw text using font rendering
    }

    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        x >= self.x as f32 
            && x < (self.x + self.width as i32) as f32
            && y >= self.y as f32
            && y < (self.y + self.height as i32) as f32
    }
}




