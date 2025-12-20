use crate::engine::{Application, EngineState};
use crate::shapes::basic_shapes;

const BACKGROUND_COLOR: (u8, u8, u8) = (10, 10, 10); // Very dark background
const LED_ON_COLOR: (u8, u8, u8) = (255, 0, 0); // Red for on LEDs
const LED_OFF_COLOR: (u8, u8, u8) = (20, 20, 20); // Very dim for off LEDs

pub struct Leds {
    led_states: Vec<Vec<bool>>, // 2D array of LED states (on/off)
    led_size_pixels: f32, // Size of each LED in pixels
    grid_cols: usize, // Number of columns in the LED grid
    grid_rows: usize, // Number of rows in the LED grid
    initialized: bool, // Whether we've initialized the random pattern
}

impl Leds {
    pub fn new() -> Self {
        Self {
            led_states: Vec::new(),
            led_size_pixels: 0.0,
            grid_cols: 0,
            grid_rows: 0,
            initialized: false,
        }
    }

    /// Calculate LED grid dimensions based on screen size
    /// Ensures all LEDs are fully contained within the screen bounds
    fn calculate_grid(&mut self, width: f32, height: f32) {
        // Target: each LED should be roughly 20-30 pixels
        let target_led_size = 25.0; // Target pixels per LED
        
        // Reserve space for LED radius on all sides (half LED size on each edge)
        // We'll calculate this iteratively, but start with an estimate
        let padding = target_led_size * 0.5; // Half the target LED size for padding
        let available_width = width - (padding * 2.0);
        let available_height = height - (padding * 2.0);
        
        // Calculate grid dimensions based on available space
        self.grid_cols = (available_width / target_led_size) as usize;
        self.grid_rows = (available_height / target_led_size) as usize;
        
        // Ensure minimum grid size
        self.grid_cols = self.grid_cols.max(1);
        self.grid_rows = self.grid_rows.max(1);
        
        // Calculate spacing and LED size to fit within bounds
        if self.grid_cols > 1 && self.grid_rows > 1 {
            // Calculate spacing that fits within available space
            let spacing_x = available_width / (self.grid_cols - 1) as f32;
            let spacing_y = available_height / (self.grid_rows - 1) as f32;
            let spacing = spacing_x.min(spacing_y);
            
            // LED size should be a reasonable fraction of spacing (e.g., 80%)
            self.led_size_pixels = spacing * 0.8;
            
            // Verify LEDs fit - if not, reduce grid size
            let radius = self.led_size_pixels / 2.0;
            let total_width_needed = (self.grid_cols - 1) as f32 * spacing + self.led_size_pixels;
            let total_height_needed = (self.grid_rows - 1) as f32 * spacing + self.led_size_pixels;
            
            if total_width_needed > available_width || total_height_needed > available_height {
                // Reduce grid size if needed
                if total_width_needed > available_width {
                    self.grid_cols = ((available_width - self.led_size_pixels) / spacing) as usize + 1;
                    self.grid_cols = self.grid_cols.max(1);
                }
                if total_height_needed > available_height {
                    self.grid_rows = ((available_height - self.led_size_pixels) / spacing) as usize + 1;
                    self.grid_rows = self.grid_rows.max(1);
                }
            }
        } else {
            // Single row or column case
            if self.grid_cols == 1 {
                self.led_size_pixels = available_height.min(available_width) * 0.8;
            } else {
                self.led_size_pixels = available_width.min(available_height) * 0.8;
            }
        }
        
        // Initialize or resize LED states if grid changed
        let old_rows = self.led_states.len();
        let old_cols = if old_rows > 0 { self.led_states[0].len() } else { 0 };
        
        if old_rows != self.grid_rows || old_cols != self.grid_cols {
            // Create new grid, preserving existing values where possible
            let mut new_states = vec![vec![false; self.grid_cols]; self.grid_rows];
            
            // Copy existing values if they fit
            for (row_idx, new_row) in new_states.iter_mut().enumerate() {
                if row_idx < old_rows {
                    for (col_idx, led) in new_row.iter_mut().enumerate() {
                        if col_idx < old_cols {
                            *led = self.led_states[row_idx][col_idx];
                        }
                    }
                }
            }
            
            self.led_states = new_states;
        }
    }

    /// Initialize LED states with random binary bitmap
    fn randomize_leds(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use rand::Rng;
            let mut rng = rand::rng();
            for row in &mut self.led_states {
                for led in row.iter_mut() {
                    // Generate random float and check if < 0.5
                    *led = rng.random::<f32>() < 0.5;
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            // WASM fallback: use simple pseudo-random
            let mut seed = 12345u32;
            for row in &mut self.led_states {
                for led in row.iter_mut() {
                    seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                    *led = (seed % 2) == 0; // 50% chance
                }
            }
        }
    }
}

impl Application for Leds {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let shape = state.frame.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        let buffer = state.frame_buffer_mut();
        let len = buffer.len();

        // Clear background
        for i in (0..len).step_by(4) {
            buffer[i + 0] = BACKGROUND_COLOR.0;
            buffer[i + 1] = BACKGROUND_COLOR.1;
            buffer[i + 2] = BACKGROUND_COLOR.2;
            buffer[i + 3] = 0xff;
        }

        // Calculate grid dimensions based on current screen size
        self.calculate_grid(width, height);

        // Initialize with random pattern only once on first run
        if !self.initialized {
            self.randomize_leds();
            self.initialized = true;
        }

        // Calculate spacing and center the grid
        let radius = self.led_size_pixels / 2.0;
        let padding = radius; // Padding on all sides to ensure LEDs don't get cut off
        
        let available_width = width - (padding * 2.0);
        let available_height = height - (padding * 2.0);
        
        let spacing_x = if self.grid_cols > 1 {
            available_width / (self.grid_cols - 1) as f32
        } else {
            0.0
        };
        let spacing_y = if self.grid_rows > 1 {
            available_height / (self.grid_rows - 1) as f32
        } else {
            0.0
        };
        
        // Calculate offset to center the grid
        let total_width = if self.grid_cols > 1 {
            (self.grid_cols - 1) as f32 * spacing_x + self.led_size_pixels
        } else {
            self.led_size_pixels
        };
        let total_height = if self.grid_rows > 1 {
            (self.grid_rows - 1) as f32 * spacing_y + self.led_size_pixels
        } else {
            self.led_size_pixels
        };
        
        let offset_x = padding + (available_width - total_width) / 2.0;
        let offset_y = padding + (available_height - total_height) / 2.0;

        // Draw each LED, ensuring they're all fully contained
        for (row_idx, row) in self.led_states.iter().enumerate() {
            for (col_idx, &is_on) in row.iter().enumerate() {
                // Position LEDs with padding and centering
                let center_x = if self.grid_cols > 1 {
                    offset_x + col_idx as f32 * spacing_x
                } else {
                    width / 2.0
                };
                let center_y = if self.grid_rows > 1 {
                    offset_y + row_idx as f32 * spacing_y
                } else {
                    height / 2.0
                };
                
                // Verify LED is fully within bounds before drawing
                if center_x - radius >= 0.0 && center_x + radius <= width &&
                   center_y - radius >= 0.0 && center_y + radius <= height {
                
                    let color = if is_on { LED_ON_COLOR } else { LED_OFF_COLOR };
                    
                    basic_shapes::draw_circle(
                        buffer,
                        width as u32,
                        height as u32,
                        center_x,
                        center_y,
                        radius,
                        color,
                        true, // Enable anti-aliasing
                    );
                }
            }
        }
    }

    fn on_mouse_down(&mut self, _state: &mut EngineState) {}
    fn on_mouse_up(&mut self, _state: &mut EngineState) {}
    fn on_mouse_move(&mut self, _state: &mut EngineState) {}
    fn on_key_char(&mut self, _state: &mut EngineState, _ch: char) {}
}

