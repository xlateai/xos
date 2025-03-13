use minifb::{Key, Window, WindowOptions};

const WIDTH: usize = 320;
const HEIGHT: usize = 240;

pub fn open_viewport() {
    // Create a buffer to store our pixels (using u32 for RGBA)
    let mut buffer: Vec<u32> = vec![0; WIDTH * HEIGHT];
    
    // Create a window
    let mut window = Window::new(
        "Black with White Center",
        WIDTH,
        HEIGHT,
        WindowOptions::default(),
    )
    .unwrap_or_else(|e| {
        panic!("Failed to create window: {}", e);
    });

    // Limit to ~60 fps
    // window.limit_update_rate(Some(std::time::Duration::from_micros(16600)));

    // Main loop
    while window.is_open() && !window.is_key_down(Key::Escape) {
        // Set all pixels to black (0)
        buffer.fill(0);
        
        // Set the center pixel to white (0xFFFFFF)
        let center_x = WIDTH / 2;
        let center_y = HEIGHT / 2;
        let center_idx = center_y * WIDTH + center_x;
        
        if center_idx < buffer.len() {
            buffer[center_idx] = 0xFFFFFF; // White (RGB format - minifb uses 0x00RRGGBB)
        }
        
        // Update the window with our pixel buffer
        window.update_with_buffer(&buffer, WIDTH, HEIGHT)
            .unwrap_or_else(|e| {
                panic!("Failed to update window: {}", e);
            });
    }
}