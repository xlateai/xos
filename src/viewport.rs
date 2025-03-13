use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId, WindowAttributes};
use pixels::{Pixels, SurfaceTexture};

const WIDTH: u32 = 320;
const HEIGHT: u32 = 240;

#[derive(Default)]
struct App<'a> {
    window: Option<Window>,
    pixels: Option<Pixels<'a>>,
}

impl ApplicationHandler for App<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Create a window with specific dimensions
        let window_attributes = WindowAttributes::default()
            .with_inner_size(winit::dpi::PhysicalSize::new(WIDTH, HEIGHT))
            .with_title("Black with White Center");
        
        let window = event_loop.create_window(window_attributes).unwrap();
        
        // Create a new pixel buffer
        let window_size = window.inner_size();
        let surface_texture = SurfaceTexture::new(window_size.width, window_size.height, window);
        let pixels = Pixels::new(WIDTH, HEIGHT, surface_texture).unwrap();
        
        // self.window = Some(window);
        self.pixels = Some(pixels);
        
        // Initial draw
        self.draw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                println!("The close button was pressed; stopping");
                event_loop.exit();
            },
            WindowEvent::RedrawRequested => {
                // Render the current frame
                if let Some(pixels) = &mut self.pixels {
                    if let Err(err) = pixels.render() {
                        println!("Error rendering: {}", err);
                        event_loop.exit();
                        return;
                    }
                }
                
                // Request another redraw
                self.window.as_ref().unwrap().request_redraw();
            },
            WindowEvent::Resized(new_size) => {
                // Resize the pixel buffer when the window is resized
                if let Some(pixels) = &mut self.pixels {
                    let _ = pixels.resize_surface(new_size.width, new_size.height);
                }
            },
            _ => (),
        }
    }
}

impl App<'_> {
    fn draw(&mut self) {
        if let Some(pixels) = &mut self.pixels {
            let frame = pixels.frame_mut();
            
            // Set all pixels to black
            for pixel in frame.chunks_exact_mut(4) {
                pixel[0] = 0;    // R
                pixel[1] = 0;    // G
                pixel[2] = 0;    // B
                pixel[3] = 255;  // A
            }
            
            // Set the center pixel to white
            let center_x = WIDTH / 2;
            let center_y = HEIGHT / 2;
            let center_idx = (center_y * WIDTH + center_x) as usize * 4;
            
            if center_idx + 3 < frame.len() {
                frame[center_idx] = 255;     // R
                frame[center_idx + 1] = 255; // G
                frame[center_idx + 2] = 255; // B
                frame[center_idx + 3] = 255; // A
            }
        }
    }
}

pub fn open_viewport() {
    let event_loop = EventLoop::new().unwrap();
    
    // Use Wait for efficiency since we're not continuously rendering
    event_loop.set_control_flow(ControlFlow::Wait);
    
    let mut app = App::default();
    let _ = event_loop.run_app(&mut app);
}