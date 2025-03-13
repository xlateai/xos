use pixels::{Pixels, SurfaceTexture};
use winapi::um::winuser;
use winit::{
    dpi::{LogicalPosition, LogicalSize},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{WindowBuilder},
};

const WINDOW_WIDTH: u32 = 256;
const WINDOW_HEIGHT: u32 = 256;

pub fn open_four_windows() {
    // Create a single event loop (must be on main thread)
    let event_loop = EventLoop::new();

    // Get primary monitor dimensions
    let (screen_width, screen_height) = unsafe {
        let desktop_window = winuser::GetDesktopWindow();
        let mut rect = std::mem::zeroed();
        winuser::GetWindowRect(desktop_window, &mut rect);
        ((rect.right - rect.left) as u32, (rect.bottom - rect.top) as u32)
    };

    // Calculate center of screen
    let center_x = screen_width / 2;
    let center_y = screen_height / 2;

    // Calculate positions for each quadrant
    let positions = [
        // Top-left quadrant
        (center_x - WINDOW_WIDTH, center_y - WINDOW_HEIGHT),
        // Top-right quadrant
        (center_x, center_y - WINDOW_HEIGHT),
        // Bottom-left quadrant
        (center_x - WINDOW_WIDTH, center_y),
        // Bottom-right quadrant
        (center_x, center_y),
    ];

    // Create windows and pixel buffers
    let mut windows = Vec::new();
    let mut pixel_buffers = Vec::new();

    for (i, (x, y)) in positions.iter().enumerate() {
        // Create window
        let window = WindowBuilder::new()
            .with_title(format!("Window {} - White Pixel", i + 1))
            .with_inner_size(LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))
            .with_resizable(false)
            .with_position(LogicalPosition::new(*x, *y))
            .with_decorations(true)
            .build(&event_loop)
            .unwrap();

        // Create pixel buffer
        let surface_texture = SurfaceTexture::new(WINDOW_WIDTH, WINDOW_HEIGHT, &window);
        let pixels = Pixels::new(WINDOW_WIDTH, WINDOW_HEIGHT, surface_texture).unwrap();

        windows.push(window);
        pixel_buffers.push(pixels);
    }

    // Run the event loop for all windows
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                window_id,
                event: WindowEvent::CloseRequested,
                ..
            } => {
                // Check if all windows have been closed
                if windows.iter().all(|window| window.id() != window_id) {
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::RedrawRequested(window_id) => {
                // Find which window requested redraw
                for (window, pixels) in windows.iter().zip(pixel_buffers.iter_mut()) {
                    if window.id() == window_id {
                        let frame = pixels.frame_mut();
                        
                        // Fill entire frame with black
                        for pixel in frame.chunks_exact_mut(4) {
                            pixel[0] = 0x00; // R
                            pixel[1] = 0x00; // G
                            pixel[2] = 0x00; // B
                            pixel[3] = 0xff; // A
                        }
                        
                        // Set center pixel to white
                        let index = ((128 * WINDOW_WIDTH as usize) + 128) * 4;
                        frame[index] = 0xff;     // R
                        frame[index + 1] = 0xff; // G
                        frame[index + 2] = 0xff; // B
                        frame[index + 3] = 0xff; // A
                        
                        // Render the frame
                        if let Err(e) = pixels.render() {
                            eprintln!("pixels.render() failed: {}", e);
                            *control_flow = ControlFlow::Exit;
                        }
                    }
                }
            }
            Event::MainEventsCleared => {
                // Request redraw for all windows
                for window in windows.iter() {
                    window.request_redraw();
                }
            }
            _ => (),
        }
    });
}