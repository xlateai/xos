use crate::engine::{Application, EngineState};
use crate::video::webcam;
use crate::ui::selector::Selector;

pub struct CameraApp {
    last_width: u32,
    last_height: u32,
    selector: Selector,
    camera_names: Vec<String>,
    cameras_initialized: bool,
}

impl CameraApp {
    pub fn new() -> Self {
        Self {
            last_width: 0,
            last_height: 0,
            selector: Selector::new(vec![]), // Will be populated in setup
            camera_names: vec![],
            cameras_initialized: false,
        }
    }
    
    fn initialize_cameras(&mut self) {
        if self.cameras_initialized {
            return;
        }
        
        let count = webcam::get_camera_count();
        self.camera_names.clear();
        
        for i in 0..count {
            if let Some(name) = webcam::get_camera_name(i) {
                self.camera_names.push(name);
            } else {
                self.camera_names.push(format!("Camera {}", i));
            }
        }
        
        if !self.camera_names.is_empty() {
            // Recreate selector with camera names
            self.selector = Selector::new(self.camera_names.clone());
            self.cameras_initialized = true;
        }
    }
    
    fn draw_camera_button(&self, state: &mut EngineState, width: u32, height: u32) {
        let button_width = 200.0;
        let button_height = 50.0;
        let button_x = (width as f32 - button_width) / 2.0;
        let button_y = height as f32 - button_height - 20.0;
        
        let mouse_x = state.mouse.x;
        let mouse_y = state.mouse.y;
        
        let is_hovered = mouse_x >= button_x && mouse_x <= button_x + button_width &&
                        mouse_y >= button_y && mouse_y <= button_y + button_height;
        
        let buffer = state.frame_buffer_mut();
        let bg_color = if is_hovered {
            (80, 80, 100)
        } else {
            (60, 60, 80)
        };
        
        // Draw button background
        for py in (button_y as i32)..(button_y as i32 + button_height as i32) {
            for px in (button_x as i32)..(button_x as i32 + button_width as i32) {
                if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                    let idx = ((py as u32 * width + px as u32) * 4) as usize;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = bg_color.0;
                        buffer[idx + 1] = bg_color.1;
                        buffer[idx + 2] = bg_color.2;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
        }
        
        // Draw button text (simple "Camera" text)
        let text = if self.cameras_initialized {
            format!("Camera ({})", webcam::get_current_camera_index() + 1)
        } else {
            "Camera".to_string()
        };
        
        // Simple text rendering - just draw "Camera" text
        // For now, we'll use a simple approach - you could use the text renderer here
        let text_x = button_x as i32 + 10;
        let text_y = button_y as i32 + 30;
        
        // Draw simple "Camera" text (very basic)
        let text_bytes = text.as_bytes();
        for (i, &_byte) in text_bytes.iter().enumerate() {
            let char_x = text_x + (i as i32 * 8);
            if char_x < (button_x + button_width) as i32 && char_x >= 0 {
                // Draw a simple character representation (very basic)
                // In a real implementation, you'd use a font renderer
                for y in 0..10 {
                    for x in 0..6 {
                        let px = char_x + x;
                        let py = text_y + y;
                        if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                            let idx = ((py as u32 * width + px as u32) * 4) as usize;
                            if idx + 3 < buffer.len() {
                                buffer[idx + 0] = 255;
                                buffer[idx + 1] = 255;
                                buffer[idx + 2] = 255;
                                buffer[idx + 3] = 0xff;
                            }
                        }
                    }
                }
            }
        }
    }

    fn capture_frame(&self, width: u32, height: u32) -> Vec<u8> {
        let (native_w, native_h) = webcam::get_resolution();
        
        // If camera is not ready yet (resolution is 0x0), return black frame
        if native_w == 0 || native_h == 0 {
            return vec![0; (width * height * 3) as usize];
        }
        
        let frame = webcam::get_frame();
        
        // If no frame data available yet, return black frame
        if frame.is_empty() {
            return vec![0; (width * height * 3) as usize];
        }

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
        let shape = state.frame.shape();
        self.last_width = shape[1] as u32;
        self.last_height = shape[0] as u32;

        webcam::init_camera();
        
        // Initialize cameras list after a short delay to allow camera to initialize
        // We'll do this in tick instead
        
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let shape = state.frame.shape();
        let width = shape[1] as u32;
        let height = shape[0] as u32;

        if width != self.last_width || height != self.last_height {
            self.last_width = width;
            self.last_height = height;
        }
        
        // Initialize cameras if not done yet (wait for camera to be ready)
        if !self.cameras_initialized && webcam::get_resolution() != (0, 0) {
            self.initialize_cameras();
        }
        
        // Update selector
        self.selector.update(width as f32, height as f32);

        let rgb_frame = self.capture_frame(width, height);

        // Fix: Get a mutable reference to the buffer
        let rgba = state.frame_buffer_mut();
        rgba.fill(0); // Optional: black background for areas not filled
        Self::copy_rgb_to_rgba(&rgb_frame, rgba);
        
        // Render selector on top
        self.selector.render(state);
        
        // Draw camera selector button at bottom center
        self.draw_camera_button(state, width, height);
    }
    
    fn on_mouse_down(&mut self, state: &mut EngineState) {
        let shape = state.frame.shape();
        let width = shape[1] as u32;
        let height = shape[0] as u32;
        
        // Check if selector handled the click
        if self.selector.on_mouse_down(state) {
            // Camera was selected
            if let Some(selected_idx) = self.selector.selected_index() {
                if webcam::switch_camera(selected_idx) {
                    crate::print(&format!("[Camera] Switched to camera: {}", self.camera_names[selected_idx]));
                }
            }
            return;
        }
        
        // Check if camera button was clicked
        let button_width = 200.0;
        let button_height = 50.0;
        let button_x = (width as f32 - button_width) / 2.0;
        let button_y = height as f32 - button_height - 20.0;
        
        let mouse_x = state.mouse.x;
        let mouse_y = state.mouse.y;
        
        if mouse_x >= button_x && mouse_x <= button_x + button_width &&
           mouse_y >= button_y && mouse_y <= button_y + button_height {
            self.selector.toggle();
        }
    }
    
    fn on_mouse_up(&mut self, _state: &mut EngineState) {
        // Empty implementation
    }
    
    fn on_mouse_move(&mut self, _state: &mut EngineState) {
        // Empty implementation
    }
}