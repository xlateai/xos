use crate::engine::{Application, EngineState};
use crate::video::webcam;

pub struct CameraApp {
    last_width: u32,
    last_height: u32,
}

impl CameraApp {
    pub fn new() -> Self {
        Self {
            last_width: 0,
            last_height: 0,
        }
    }

    fn capture_frame(&self, width: u32, height: u32) -> Vec<u8> {
        let (native_w, native_h) = webcam::get_resolution();
        let frame = webcam::get_frame();

        let (dst_w, dst_h, offset_x, offset_y) =
            Self::calculate_dimensions(native_w, native_h, width, height);

        let resized = Self::resize_rgb(&frame, native_w, native_h, dst_w, dst_h);

        // Allocate output RGB buffer to fit entire canvas
        let mut result = vec![0; (width * height * 3) as usize];

        Self::copy_with_offset(
            &mut result,
            &resized,
            width,
            height,
            dst_w,
            dst_h,
            offset_x,
            offset_y,
        );

        result
    }

    fn calculate_dimensions(
        src_w: u32,
        src_h: u32,
        dst_w: u32,
        dst_h: u32,
    ) -> (u32, u32, u32, u32) {
        let src_aspect = src_w as f32 / src_h as f32;
        let dst_aspect = dst_w as f32 / dst_h as f32;

        if src_aspect > dst_aspect {
            let fit_w = dst_w;
            let fit_h = (dst_w as f32 / src_aspect) as u32;
            let offset_y = (dst_h - fit_h) / 2;
            (fit_w, fit_h, 0, offset_y)
        } else {
            let fit_h = dst_h;
            let fit_w = (dst_h as f32 * src_aspect) as u32;
            let offset_x = (dst_w - fit_w) / 2;
            (fit_w, fit_h, offset_x, 0)
        }
    }

    fn copy_with_offset(
        dst: &mut [u8],
        src: &[u8],
        dst_width: u32,
        dst_height: u32,
        src_width: u32,
        src_height: u32,
        offset_x: u32,
        offset_y: u32,
    ) {
        for y in 0..src_height {
            if y + offset_y >= dst_height {
                continue;
            }

            for x in 0..src_width {
                if x + offset_x >= dst_width {
                    continue;
                }

                let src_idx = ((y * src_width + x) * 3) as usize;
                let dst_idx = (((y + offset_y) * dst_width + (x + offset_x)) * 3) as usize;

                if src_idx + 2 < src.len() && dst_idx + 2 < dst.len() {
                    dst[dst_idx] = src[src_idx];
                    dst[dst_idx + 1] = src[src_idx + 1];
                    dst[dst_idx + 2] = src[src_idx + 2];
                }
            }
        }
    }

    fn resize_rgb(
        src: &[u8],
        src_w: u32,
        src_h: u32,
        dst_w: u32,
        dst_h: u32,
    ) -> Vec<u8> {
        let mut dst = vec![0; (dst_w * dst_h * 3) as usize];

        if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
            return dst;
        }

        for y in 0..dst_h {
            for x in 0..dst_w {
                let src_x = x * src_w / dst_w;
                let src_y = y * src_h / dst_h;
                let src_idx = ((src_y * src_w + src_x) * 3) as usize;
                let dst_idx = ((y * dst_w + x) * 3) as usize;

                if src_idx + 2 < src.len() && dst_idx + 2 < dst.len() {
                    dst[dst_idx] = src[src_idx];
                    dst[dst_idx + 1] = src[src_idx + 1];
                    dst[dst_idx + 2] = src[src_idx + 2];
                }
            }
        }

        dst
    }

    fn copy_rgb_to_rgba(src_rgb: &[u8], dst_rgba: &mut [u8]) {
        let mut j = 0;
        for i in 0..(src_rgb.len() / 3) {
            if j + 3 >= dst_rgba.len() {
                break;
            }
            dst_rgba[j] = src_rgb[i * 3];
            dst_rgba[j + 1] = src_rgb[i * 3 + 1];
            dst_rgba[j + 2] = src_rgb[i * 3 + 2];
            dst_rgba[j + 3] = 0xFF;
            j += 4;
        }
    }
}

impl Application for CameraApp {
    fn setup(&mut self, state: &mut EngineState) -> Result<(), String> {
        self.last_width = state.frame.width;
        self.last_height = state.frame.height;

        webcam::init_camera();
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let width = state.frame.width;
        let height = state.frame.height;

        if width != self.last_width || height != self.last_height {
            self.last_width = width;
            self.last_height = height;
        }

        let rgb_frame = self.capture_frame(width, height);

        // Fix: Get a mutable reference to the buffer
        let rgba = &mut state.frame.buffer;
        rgba.fill(0); // Optional: black background for areas not filled
        Self::copy_rgb_to_rgba(&rgb_frame, rgba);
    }

    fn on_mouse_down(&mut self, _state: &mut EngineState) {
        // Empty implementation
    }
    
    fn on_mouse_up(&mut self, _state: &mut EngineState) {
        // Empty implementation
    }
    
    fn on_mouse_move(&mut self, _state: &mut EngineState) {
        // Empty implementation
    }
}