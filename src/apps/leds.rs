use crate::engine::{Application, EngineState};
use crate::rasterizer::{circles, fill};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

const BACKGROUND_COLOR: (u8, u8, u8) = (10, 10, 10);
const LED_ON_COLOR: (u8, u8, u8) = (255, 0, 0);
const LED_OFF_COLOR: (u8, u8, u8) = (20, 20, 20);
/// Animation update rate (logic steps per second), independent of render rate.
const ANIM_FPS: f32 = 60.0;
const ANIM_FRAME_MS: f32 = 1000.0 / ANIM_FPS;

pub struct Leds {
    led_states: Vec<Vec<bool>>,
    led_size_pixels: f32,
    grid_cols: usize,
    grid_rows: usize,
    initialized: bool,
    row_directions: Vec<bool>,
    #[cfg(not(target_arch = "wasm32"))]
    last_update_time: Option<Instant>,
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

    fn calculate_grid(&mut self, width: f32, height: f32) {
        let target_led_size = 25.0;
        let padding = target_led_size * 0.5;
        let available_width = width - (padding * 2.0);
        let available_height = height - (padding * 2.0);

        self.grid_cols = (available_width / target_led_size).max(1.0) as usize;
        self.grid_rows = (available_height / target_led_size).max(1.0) as usize;

        if self.grid_cols > 1 && self.grid_rows > 1 {
            let spacing_x = available_width / (self.grid_cols - 1) as f32;
            let spacing_y = available_height / (self.grid_rows - 1) as f32;
            let spacing = spacing_x.min(spacing_y);
            self.led_size_pixels = spacing * 0.8;

            let total_width_needed =
                (self.grid_cols - 1) as f32 * spacing + self.led_size_pixels;
            let total_height_needed =
                (self.grid_rows - 1) as f32 * spacing + self.led_size_pixels;

            if total_width_needed > available_width {
                self.grid_cols =
                    (((available_width - self.led_size_pixels) / spacing) as usize).max(1);
            }
            if total_height_needed > available_height {
                self.grid_rows =
                    (((available_height - self.led_size_pixels) / spacing) as usize).max(1);
            }
        } else if self.grid_cols == 1 {
            self.led_size_pixels = available_height.min(available_width) * 0.8;
        } else {
            self.led_size_pixels = available_width.min(available_height) * 0.8;
        }

        let old_rows = self.led_states.len();
        let old_cols = if old_rows > 0 {
            self.led_states[0].len()
        } else {
            0
        };

        if old_rows != self.grid_rows || old_cols != self.grid_cols {
            self.led_states = vec![vec![false; self.grid_cols]; self.grid_rows];
            if self.row_directions.len() != self.grid_rows {
                self.initialize_row_directions();
            }
        }
    }

    fn initialize_row_directions(&mut self) {
        self.row_directions.clear();
        #[cfg(not(target_arch = "wasm32"))]
        {
            use rand::Rng;
            let mut rng = rand::rng();
            self.row_directions
                .extend((0..self.grid_rows).map(|_| rng.random::<f32>() < 0.5));
        }
        #[cfg(target_arch = "wasm32")]
        {
            let mut seed = 54321u32;
            for _ in 0..self.grid_rows {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                self.row_directions.push((seed % 2) == 0);
            }
        }
    }

    fn update_animation(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use rand::Rng;
            let mut rng = rand::rng();
            for (row_idx, row) in self.led_states.iter_mut().enumerate() {
                let direction_right = self.row_directions.get(row_idx).copied().unwrap_or(true);
                let len = row.len();
                if len == 0 {
                    continue;
                }
                // Shift right: each cell takes the value from its left neighbor; spawn at column 0.
                if direction_right {
                    row.copy_within(0..len - 1, 1);
                    row[0] = rng.random::<f32>() < 0.5;
                } else {
                    // Shift left: each cell takes from the right; spawn at last column.
                    row.copy_within(1..len, 0);
                    row[len - 1] = rng.random::<f32>() < 0.5;
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let mut seed = 12345u32;
            for (row_idx, row) in self.led_states.iter_mut().enumerate() {
                let direction_right = self.row_directions.get(row_idx).copied().unwrap_or(true);
                let len = row.len();
                if len == 0 {
                    continue;
                }
                if direction_right {
                    row.copy_within(0..len - 1, 1);
                    seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                    row[0] = (seed % 2) == 0;
                } else {
                    row.copy_within(1..len, 0);
                    seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                    row[len - 1] = (seed % 2) == 0;
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
        let shape = state.frame.array.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;

        fill(
            &mut state.frame,
            (BACKGROUND_COLOR.0, BACKGROUND_COLOR.1, BACKGROUND_COLOR.2, 255),
        );

        self.calculate_grid(width, height);

        if !self.initialized {
            self.initialized = true;
            #[cfg(not(target_arch = "wasm32"))]
            {
                self.last_update_time = Some(Instant::now());
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(last_time) = self.last_update_time {
                let elapsed_ms = last_time.elapsed().as_secs_f32() * 1000.0;
                if elapsed_ms >= ANIM_FRAME_MS {
                    self.update_animation();
                    self.last_update_time = Some(Instant::now());
                }
            } else {
                self.last_update_time = Some(Instant::now());
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.update_animation();
        }

        let radius = self.led_size_pixels * 0.5;
        let padding = radius;
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

        let offset_x = padding + (available_width - total_width) * 0.5;
        let offset_y = padding + (available_height - total_height) * 0.5;

        let n = self.grid_cols * self.grid_rows;
        let mut centers: Vec<(f32, f32)> = Vec::with_capacity(n);
        let mut colors: Vec<[u8; 4]> = Vec::with_capacity(n);

        for (row_idx, row) in self.led_states.iter().enumerate() {
            for (col_idx, &is_on) in row.iter().enumerate() {
                let center_x = if self.grid_cols > 1 {
                    offset_x + col_idx as f32 * spacing_x
                } else {
                    width * 0.5
                };
                let center_y = if self.grid_rows > 1 {
                    offset_y + row_idx as f32 * spacing_y
                } else {
                    height * 0.5
                };

                if center_x - radius >= 0.0
                    && center_x + radius <= width
                    && center_y - radius >= 0.0
                    && center_y + radius <= height
                {
                    let c = if is_on {
                        [LED_ON_COLOR.0, LED_ON_COLOR.1, LED_ON_COLOR.2, 255]
                    } else {
                        [LED_OFF_COLOR.0, LED_OFF_COLOR.1, LED_OFF_COLOR.2, 255]
                    };
                    centers.push((center_x, center_y));
                    colors.push(c);
                }
            }
        }

        let radii = [radius];
        let _ = circles(&mut state.frame, &centers, &radii, &colors);
    }

    fn on_mouse_down(&mut self, _state: &mut EngineState) {}
    fn on_mouse_up(&mut self, _state: &mut EngineState) {}
    fn on_mouse_move(&mut self, _state: &mut EngineState) {}
    fn on_key_char(&mut self, _state: &mut EngineState, _ch: char) {}
}
