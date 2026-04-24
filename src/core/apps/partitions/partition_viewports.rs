use super::partition::{Partition, PartitionData};
use crate::rasterizer::text::fonts;
use crate::rasterizer::text::text_rasterization::TextRasterizer;
use std::cell::RefCell;

const COLOR_A: (u8, u8, u8) = (100, 150, 255);
const COLOR_B: (u8, u8, u8) = (255, 150, 100);
const COLOR_C: (u8, u8, u8) = (150, 255, 100);
const TEXT_COLOR: (u8, u8, u8) = (255, 255, 255);

pub struct PartitionA {
    pub data: PartitionData,
    text_rasterizer: RefCell<TextRasterizer>,
}

impl PartitionA {
    pub fn new(left: f32, right: f32, top: f32, bottom: f32) -> Self {
        let font = fonts::default_font();
        let mut text_rasterizer = TextRasterizer::new(font, 24.0);
        text_rasterizer.set_text("Hello, welcome to the partition view display demo. I hope this finds you well, as it's become a priority of xos to make simple and powerful design tooling available at all levels of the system.".to_string());

        Self {
            data: PartitionData::new(left, right, top, bottom, COLOR_A),
            text_rasterizer: RefCell::new(text_rasterizer),
        }
    }
}

impl Partition for PartitionA {
    fn data(&self) -> &PartitionData {
        &self.data
    }

    fn data_mut(&mut self) -> &mut PartitionData {
        &mut self.data
    }

    fn draw(&self, buffer: &mut [u8], width: u32, height: u32) {
        let w = width as f32;
        let h = height as f32;

        let x0 = (self.data.left * w).round().clamp(0.0, w) as i32;
        let x1 = (self.data.right * w).round().clamp(0.0, w) as i32;
        let y0 = (self.data.top * h).round().clamp(0.0, h) as i32;
        let y1 = (self.data.bottom * h).round().clamp(0.0, h) as i32;

        let rect_w = (x1 - x0).max(0) as u32;
        let rect_h = (y1 - y0).max(0) as u32;

        // Draw the partition background
        for dy in 0..rect_h {
            for dx in 0..rect_w {
                let sx = x0 + dx as i32;
                let sy = y0 + dy as i32;

                if sx >= 0 && sy >= 0 && (sx as u32) < width && (sy as u32) < height {
                    let idx = ((sy as u32 * width + sx as u32) * 4) as usize;
                    buffer[idx + 0] = COLOR_A.0;
                    buffer[idx + 1] = COLOR_A.1;
                    buffer[idx + 2] = COLOR_A.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }

        // Draw centered text
        let mut text_rasterizer = self.text_rasterizer.borrow_mut();
        text_rasterizer.tick(rect_w as f32, rect_h as f32);

        // Calculate text bounding box
        let mut text_min_x = f32::MAX;
        let mut text_max_x = f32::MIN;
        let mut text_min_y = f32::MAX;
        let mut text_max_y = f32::MIN;

        for character in &text_rasterizer.characters {
            text_min_x = text_min_x.min(character.x);
            text_max_x = text_max_x.max(character.x + character.width);
            text_min_y = text_min_y.min(character.y);
            text_max_y = text_max_y.max(character.y + character.height);
        }

        if text_rasterizer.characters.is_empty() {
            return;
        }

        let text_width = text_max_x - text_min_x;
        let text_height = text_max_y - text_min_y;

        // Calculate centering offset
        let offset_x = (rect_w as f32 - text_width) / 2.0 - text_min_x;
        let offset_y = (rect_h as f32 - text_height) / 2.0 - text_min_y;

        // Draw each character
        for character in &text_rasterizer.characters {
            let px = x0 + (character.x + offset_x) as i32;
            let py = y0 + (character.y + offset_y) as i32;

            for y in 0..character.metrics.height {
                for x in 0..character.metrics.width {
                    let val = character.bitmap[y * character.metrics.width + x];

                    if val == 0 {
                        continue;
                    }

                    let sx = px + x as i32;
                    let sy = py + y as i32;

                    if sx >= 0 && sx < width as i32 && sy >= 0 && sy < height as i32 {
                        let idx = ((sy as u32 * width + sx as u32) * 4) as usize;
                        buffer[idx + 0] = ((TEXT_COLOR.0 as u16 * val as u16) / 255) as u8;
                        buffer[idx + 1] = ((TEXT_COLOR.1 as u16 * val as u16) / 255) as u8;
                        buffer[idx + 2] = ((TEXT_COLOR.2 as u16 * val as u16) / 255) as u8;
                        buffer[idx + 3] = val;
                    }
                }
            }
        }
    }
}

pub struct PartitionB {
    pub data: PartitionData,
}

impl PartitionB {
    pub fn new(left: f32, right: f32, top: f32, bottom: f32) -> Self {
        Self {
            data: PartitionData::new(left, right, top, bottom, COLOR_B),
        }
    }
}

impl Partition for PartitionB {
    fn data(&self) -> &PartitionData {
        &self.data
    }

    fn data_mut(&mut self) -> &mut PartitionData {
        &mut self.data
    }

    fn draw(&self, buffer: &mut [u8], width: u32, height: u32) {
        let w = width as f32;
        let h = height as f32;

        let x0 = (self.data.left * w).round().clamp(0.0, w) as i32;
        let x1 = (self.data.right * w).round().clamp(0.0, w) as i32;
        let y0 = (self.data.top * h).round().clamp(0.0, h) as i32;
        let y1 = (self.data.bottom * h).round().clamp(0.0, h) as i32;

        let rect_w = (x1 - x0).max(0) as u32;
        let rect_h = (y1 - y0).max(0) as u32;

        for dy in 0..rect_h {
            for dx in 0..rect_w {
                let sx = x0 + dx as i32;
                let sy = y0 + dy as i32;

                if sx >= 0 && sy >= 0 && (sx as u32) < width && (sy as u32) < height {
                    let idx = ((sy as u32 * width + sx as u32) * 4) as usize;
                    buffer[idx + 0] = COLOR_B.0;
                    buffer[idx + 1] = COLOR_B.1;
                    buffer[idx + 2] = COLOR_B.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }
    }
}

pub struct PartitionC {
    pub data: PartitionData,
}

impl PartitionC {
    pub fn new(left: f32, right: f32, top: f32, bottom: f32) -> Self {
        Self {
            data: PartitionData::new(left, right, top, bottom, COLOR_C),
        }
    }
}

impl Partition for PartitionC {
    fn data(&self) -> &PartitionData {
        &self.data
    }

    fn data_mut(&mut self) -> &mut PartitionData {
        &mut self.data
    }

    fn draw(&self, buffer: &mut [u8], width: u32, height: u32) {
        let w = width as f32;
        let h = height as f32;

        let x0 = (self.data.left * w).round().clamp(0.0, w) as i32;
        let x1 = (self.data.right * w).round().clamp(0.0, w) as i32;
        let y0 = (self.data.top * h).round().clamp(0.0, h) as i32;
        let y1 = (self.data.bottom * h).round().clamp(0.0, h) as i32;

        let rect_w = (x1 - x0).max(0) as u32;
        let rect_h = (y1 - y0).max(0) as u32;

        for dy in 0..rect_h {
            for dx in 0..rect_w {
                let sx = x0 + dx as i32;
                let sy = y0 + dy as i32;

                if sx >= 0 && sy >= 0 && (sx as u32) < width && (sy as u32) < height {
                    let idx = ((sy as u32 * width + sx as u32) * 4) as usize;
                    buffer[idx + 0] = COLOR_C.0;
                    buffer[idx + 1] = COLOR_C.1;
                    buffer[idx + 2] = COLOR_C.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }
    }
}
