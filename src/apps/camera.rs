use crate::engine::Application;
use crate::video::webcam;

pub struct CameraApp;

impl CameraApp {
    pub fn new() -> Self {
        Self
    }

    fn capture_frame(&self, width: u32, height: u32) -> Vec<u8> {
        let (native_w, native_h) = webcam::get_resolution();
        let frame = webcam::get_frame();

        if native_w == width && native_h == height {
            frame
        } else {
            Self::resize_rgb(&frame, native_w, native_h, width, height)
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
        // Convert RGB -> RGBA (adding full alpha channel)
        let mut rgba = Vec::with_capacity(pixels.len() / 3 * 4);
        for chunk in pixels.chunks(3) {
            rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 0xFF]);
        }
        rgba
    }
}

impl Application for CameraApp {
    fn setup(&mut self, _width: u32, _height: u32) -> Result<(), String> {
        webcam::init_camera();

        Ok(())
    }

    fn tick(&mut self, width: u32, height: u32) -> Vec<u8> {
        let rgb = self.capture_frame(width, height);
        Self::to_rgba(rgb)
    }

    fn on_mouse_down(&mut self, _x: f32, _y: f32) {}
}
