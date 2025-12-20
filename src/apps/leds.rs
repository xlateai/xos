use crate::engine::{Application, EngineState};
use crate::shapes::basic_shapes;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

const BACKGROUND_COLOR: (u8, u8, u8) = (10, 10, 10); // Very dark background
const LED_ON_COLOR: (u8, u8, u8) = (255, 0, 0); // Red for on LEDs
const LED_OFF_COLOR: (u8, u8, u8) = (20, 20, 20); // Very dim for off LEDs
const TARGET_FPS: f32 = 60.0; // Target frame rate for animation
const FRAME_DURATION_MS: f32 = 1000.0 / TARGET_FPS; // ~16.67ms per frame

pub struct Leds {
    led_states: Vec<Vec<bool>>, // 2D array of LED states (on/off)
    led_size_pixels: f32, // Size of each LED in pixels
    grid_cols: usize, // Number of columns in the LED grid
    grid_rows: usize, // Number of rows in the LED grid
    initialized: bool, // Whether we've initialized the random pattern
    row_directions: Vec<bool>, // Direction for each row (true = right, false = left)
    #[cfg(not(target_arch = "wasm32"))]
    last_update_time: Option<Instant>, // Last time we updated the animation
}

impl Leds {
    pub fn new() -> Self {
        Self {
            led_states: Vec::new(),
            led_size_pixels: 0.0,
            grid_cols: 0,
            grid_rows: 0,
            initialized: false,
            row_directions: Vec::new(),
            #[cfg(not(target_arch = "wasm32"))]
            last_update_time: None,
        }
    }

    /// Calculate LED grid dimensions based on screen size
    /// Ensures all LEDs are fully contained within the screen bounds
    fn calculate_grid(&mut self, width: f32, height: f32) {
        // Target: each LED should be roughly 35 pixels
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
            // Create new grid, all LEDs start off
            self.led_states = vec![vec![false; self.grid_cols]; self.grid_rows];
            
            // Initialize row directions if needed
            if self.row_directions.len() != self.grid_rows {
                self.initialize_row_directions();
            }
        }
    }
    
    /// Initialize row directions randomly (true = right, false = left)
    fn initialize_row_directions(&mut self) {
        self.row_directions.clear();
        #[cfg(not(target_arch = "wasm32"))]
        {
            use rand::Rng;
            let mut rng = rand::rng();
            for _ in 0..self.grid_rows {
                self.row_directions.push(rng.random::<f32>() < 0.5);
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            // WASM fallback: use simple pseudo-random
            let mut seed = 54321u32;
            for _ in 0..self.grid_rows {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                self.row_directions.push((seed % 2) == 0);
            }
        }
    }
    
    /// Update animation at 60fps - shift rows and spawn new bits
    /// Update animation at 60fps - shift rows and spawn new bits
    fn update_animation(&mut self) {
        for (row_idx, row) in self.led_states.iter_mut().enumerate() {
            let direction_right = self.row_directions.get(row_idx).copied().unwrap_or(true);
            
            if direction_right {
                // Shift right: move all bits one position to the right
                // Spawn new random bit on the left
                for col_idx in (1..row.len()).rev() {
                    row[col_idx] = row[col_idx - 1];
                }
                // Spawn random bit on the left
                #[cfg(not(target_arch = "wasm32"))]
                {
                    use rand::Rng;
                    let mut rng = rand::rng();
                    row[0] = rng.random::<f32>() < 0.5;
                }
                #[cfg(target_arch = "wasm32")]
                {
                    // WASM fallback: use simple pseudo-random
                    static mut SEED: u32 = 12345;
                    unsafe {
                        SEED = SEED.wrapping_mul(1103515245).wrapping_add(12345);
                        row[0] = (SEED % 2) == 0;
                    }
                }
            } else {
                // Shift left: move all bits one position to the left
                // Spawn new random bit on the right
                for col_idx in 0..(row.len() - 1) {
                    row[col_idx] = row[col_idx + 1];
                }
                // Spawn random bit on the right
                #[cfg(not(target_arch = "wasm32"))]
                {
                    use rand::Rng;
                    let mut rng = rand::rng();
                    let last_idx = row.len() - 1;
                    row[last_idx] = rng.random::<f32>() < 0.5;
                }
                #[cfg(target_arch = "wasm32")]
                {
                    // WASM fallback: use simple pseudo-random
                    static mut SEED: u32 = 12345;
                    unsafe {
                        SEED = SEED.wrapping_mul(1103515245).wrapping_add(12345);
                        let last_idx = row.len() - 1;
                        row[last_idx] = (SEED % 2) == 0;
                    }
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

        // Initialize only once on first run - all LEDs start off
        if !self.initialized {
            // All LEDs are already false (off) from calculate_grid
            self.initialize_row_directions();
            self.initialized = true;
            #[cfg(not(target_arch = "wasm32"))]
            {
                self.last_update_time = Some(Instant::now());
            }
        }
        
        // Update animation at 30fps
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(last_time) = self.last_update_time {
                let elapsed_ms = last_time.elapsed().as_secs_f32() * 1000.0;
                if elapsed_ms >= FRAME_DURATION_MS {
                    self.update_animation();
                    self.last_update_time = Some(Instant::now());
                }
            } else {
                self.last_update_time = Some(Instant::now());
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            // WASM: update every tick (engine handles frame rate)
            self.update_animation();
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

