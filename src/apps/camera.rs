use crate::engine::Application;
use nokhwa::{
    pixel_format::RgbFormat,
    utils::{CameraIndex, RequestedFormat, RequestedFormatType},
    Camera,
};

pub struct CameraApp {
    cam: Camera,
}

impl CameraApp {
    pub fn new() -> Self {
        let index = CameraIndex::Index(0);
        let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestResolution);
        let cam = Camera::new(index, requested).expect("Failed to open camera");
        Self { cam }
    }

    fn capture_frame(&mut self, width: u32, height: u32) -> Vec<u8> {
        match self.cam.frame() {
            Ok(frame) => {
                let decoded = frame.decode_image::<RgbFormat>().unwrap();
                let (w, h) = (decoded.width(), decoded.height());
                let buffer = decoded.into_vec();

                // Resize if needed
                if w == width && h == height {
                    buffer
                } else {
                    Self::resize_rgb(&buffer, w, h, width, height)
                }
            }
            Err(_) => vec![0; (width * height * 3) as usize],
        }
    }

    fn resize_rgb(src: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
        let mut dst = vec![0; (dst_w * dst_h * 3) as usize];
        for y in 0..dst_h {
            for x in 0..dst_w {
                let src_x = x * src_w / dst_w;
                let src_y = y * src_h / dst_h;
                let src_idx = ((src_y * src_w + src_x) * 3) as usize;
                let dst_idx = ((y * dst_w + x) * 3) as usize;
                dst[dst_idx..dst_idx + 3].copy_from_slice(&src[src_idx..src_idx + 3]);
            }
        }
        dst
    }

    fn to_rgba(pixels: Vec<u8>) -> Vec<u8> {
        // Convert RGB to RGBA (adding alpha)
        let mut rgba = Vec::with_capacity(pixels.len() / 3 * 4);
        for chunk in pixels.chunks(3) {
            rgba.push(chunk[0]);
            rgba.push(chunk[1]);
            rgba.push(chunk[2]);
            rgba.push(0xFF);
        }
        rgba
    }
}

impl Application for CameraApp {
    fn setup(&mut self, _width: u32, _height: u32) -> Result<(), String> {
        self.cam.open_stream().map_err(|e| e.to_string())?;
        Ok(())
    }

    fn tick(&mut self, width: u32, height: u32) -> Vec<u8> {
        let rgb = self.capture_frame(width, height);
        Self::to_rgba(rgb)
    }

    fn on_mouse_down(&mut self, _x: f32, _y: f32) {}
}
