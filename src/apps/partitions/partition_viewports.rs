use super::partition::{Partition, PartitionData};

const COLOR_A: (u8, u8, u8) = (100, 150, 255);
const COLOR_B: (u8, u8, u8) = (255, 150, 100);
const COLOR_C: (u8, u8, u8) = (150, 255, 100);

pub struct PartitionA {
    pub data: PartitionData,
}

impl PartitionA {
    pub fn new(left: f32, right: f32, top: f32, bottom: f32) -> Self {
        Self {
            data: PartitionData::new(left, right, top, bottom, COLOR_A),
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
