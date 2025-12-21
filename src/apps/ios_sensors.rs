use crate::engine::{Application, EngineState};
use crate::sensors::{Magnetometer, MagnetometerReading};
use crate::apps::text::geometric::GeometricText;
use fontdue::{Font, FontSettings};
use std::time::{Instant, Duration};

const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);
const TEXT_COLOR: (u8, u8, u8) = (255, 255, 255);

pub struct IosSensorsApp {
    magnetometer: Option<Magnetometer>,
    magnitude_text: GeometricText,
    coordinates_text: GeometricText,
    count_text: GeometricText,
    rate_text: GeometricText,
    last_reading: Option<MagnetometerReading>,
    // For calculating readings per second
    last_rate_calc_time: Instant,
    readings_since_last_calc: u64,
    readings_per_second: f64,
}

impl IosSensorsApp {
    pub fn new() -> Self {
        let font_bytes = include_bytes!("../../assets/JetBrainsMono-Regular.ttf") as &[u8];
        
        // Font sizes - smaller and more reasonable
        let magnitude_font_size = if cfg!(target_os = "ios") {
            64.0
        } else {
            48.0
        };
        
        let coordinates_font_size = if cfg!(target_os = "ios") {
            36.0
        } else {
            28.0
        };
        
        let small_font_size = if cfg!(target_os = "ios") {
            24.0
        } else {
            20.0
        };

        // Load font multiple times since Font doesn't implement Clone
        let magnitude_font = Font::from_bytes(font_bytes, FontSettings::default())
            .expect("Failed to load font");
        let coordinates_font = Font::from_bytes(font_bytes, FontSettings::default())
            .expect("Failed to load font");
        let count_font = Font::from_bytes(font_bytes, FontSettings::default())
            .expect("Failed to load font");
        let rate_font = Font::from_bytes(font_bytes, FontSettings::default())
            .expect("Failed to load font");

        let magnitude_text = GeometricText::new(magnitude_font, magnitude_font_size);
        let coordinates_text = GeometricText::new(coordinates_font, coordinates_font_size);
        let count_text = GeometricText::new(count_font, small_font_size);
        let rate_text = GeometricText::new(rate_font, small_font_size);

        Self {
            magnetometer: None,
            magnitude_text,
            coordinates_text,
            count_text,
            rate_text,
            last_reading: None,
            last_rate_calc_time: Instant::now(),
            readings_since_last_calc: 0,
            readings_per_second: 0.0,
        }
    }
}

impl Application for IosSensorsApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        #[cfg(not(target_os = "ios"))]
        {
            return Err("This app only works on iOS devices".to_string());
        }

        #[cfg(target_os = "ios")]
        {
            // Try to initialize magnetometer, but don't fail if it doesn't work
            // The app will just show "No reading" instead
            crate::print("Attempting to initialize magnetometer...");
            
            // Wrap in a catch-all to prevent any panics from crashing the app
            let magnetometer_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                Magnetometer::new()
            }));
            
            match magnetometer_result {
                Ok(Ok(magnetometer)) => {
                    crate::print("✅ Magnetometer initialized successfully");
                    self.magnetometer = Some(magnetometer);
                }
                Ok(Err(e)) => {
                    crate::print(&format!("⚠️ Magnetometer initialization failed: {}. App will continue without sensor data.", e));
                    self.magnetometer = None;
                }
                Err(_) => {
                    crate::print("⚠️ Magnetometer initialization panicked. App will continue without sensor data.");
                    self.magnetometer = None;
                }
            }
            
            Ok(())
        }
    }

    fn tick(&mut self, state: &mut EngineState) {
        let shape = state.frame.array.shape();
        let width = shape[1] as u32;
        let height = shape[0] as u32;
        let buffer = state.frame_buffer_mut();

        // Clear background
        for pixel in buffer.chunks_exact_mut(4) {
            pixel[0] = BACKGROUND_COLOR.0;
            pixel[1] = BACKGROUND_COLOR.1;
            pixel[2] = BACKGROUND_COLOR.2;
            pixel[3] = 255;
        }

        // Drain all readings since last tick (batch read)
        if let Some(mag) = &mut self.magnetometer {
            // Wrap in catch_unwind in case drain panics
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let batch = mag.drain_readings();
                // Update last_reading with the most recent reading from the batch
                if let Some(latest) = batch.last() {
                    self.last_reading = Some(*latest);
                }
                // Count readings for rate calculation
                self.readings_since_last_calc += batch.len() as u64;
            }));
        }

        // Calculate readings per second (update every second)
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_rate_calc_time);
        if elapsed >= Duration::from_secs(1) {
            self.readings_per_second = self.readings_since_last_calc as f64 / elapsed.as_secs_f64();
            self.readings_since_last_calc = 0;
            self.last_rate_calc_time = now;
        }

        // Get total readings
        let total_readings = self.magnetometer.as_ref()
            .map(|m| m.get_total_readings())
            .unwrap_or(0);

        // Update magnitude text (4 decimals)
        if let Some(reading) = self.last_reading {
            let magnitude = reading.magnitude();
            self.magnitude_text.set_text(format!("{:.4}uT", magnitude));
            
            // Update coordinates text (X, Y, Z on same line, 1 decimal each)
            self.coordinates_text.set_text(format!(
                "X: {:.1}, Y: {:.1}, Z: {:.1}",
                reading.x, reading.y, reading.z
            ));
        } else {
            self.magnitude_text.set_text("No reading".to_string());
            self.coordinates_text.set_text("X: --, Y: --, Z: --".to_string());
        }
        self.magnitude_text.tick(width as f32, height as f32);
        self.coordinates_text.tick(width as f32, height as f32);

        // Update count text
        let count_text_str = format!("{} readings", total_readings);
        self.count_text.set_text(count_text_str);
        self.count_text.tick(width as f32, height as f32);

        // Update rate text
        let rate_text_str = format!("{:.1} readings per second", self.readings_per_second);
        self.rate_text.set_text(rate_text_str);
        self.rate_text.tick(width as f32, height as f32);

        // Calculate positions for centering - vertically centered layout
        let center_y = height as f32 / 2.0;
        let line_spacing = 50.0;
        
        // Magnitude row (top, 4 decimals)
        let magnitude_text_width = if let Some(last_char) = self.magnitude_text.characters.last() {
            last_char.x + last_char.metrics.advance_width
        } else {
            0.0
        };
        let magnitude_x = (width as f32 - magnitude_text_width) / 2.0;
        let magnitude_y = center_y - line_spacing * 1.5;
        
        // Coordinates row (X, Y, Z on same line, 1 decimal each)
        let coordinates_text_width = if let Some(last_char) = self.coordinates_text.characters.last() {
            last_char.x + last_char.metrics.advance_width
        } else {
            0.0
        };
        let coordinates_x = (width as f32 - coordinates_text_width) / 2.0;
        let coordinates_y = center_y;
        
        // Count row
        let count_text_width = if let Some(last_char) = self.count_text.characters.last() {
            last_char.x + last_char.metrics.advance_width
        } else {
            0.0
        };
        let count_x = (width as f32 - count_text_width) / 2.0;
        let count_y = center_y + line_spacing;
        
        // Rate row
        let rate_text_width = if let Some(last_char) = self.rate_text.characters.last() {
            last_char.x + last_char.metrics.advance_width
        } else {
            0.0
        };
        let rate_x = (width as f32 - rate_text_width) / 2.0;
        let rate_y = center_y + line_spacing * 1.5;

        // Draw all text
        self.draw_text_geometric(buffer, width, height, &self.magnitude_text, magnitude_x, magnitude_y);
        self.draw_text_geometric(buffer, width, height, &self.coordinates_text, coordinates_x, coordinates_y);
        self.draw_text_geometric(buffer, width, height, &self.count_text, count_x, count_y);
        self.draw_text_geometric(buffer, width, height, &self.rate_text, rate_x, rate_y);
    }

    fn on_mouse_down(&mut self, _state: &mut EngineState) {}
    fn on_mouse_up(&mut self, _state: &mut EngineState) {}
    fn on_mouse_move(&mut self, _state: &mut EngineState) {}
}

impl IosSensorsApp {
    fn draw_text_geometric(&self, buffer: &mut [u8], width: u32, height: u32, text_engine: &GeometricText, offset_x: f32, offset_y: f32) {
        for character in &text_engine.characters {
            let px = (character.x + offset_x) as i32;
            let py = (character.y + offset_y) as i32;

            for y in 0..character.metrics.height {
                for x in 0..character.metrics.width {
                    let val = character.bitmap[y * character.metrics.width + x];
                    if val == 0 {
                        continue;
                    }

                    let sx = px + x as i32;
                    let sy = py + y as i32;

                    if sx >= 0 && sx < width as i32 && sy >= 0 && sy < height as i32 {
                        let idx = ((sy as u32 * width + sx as u32) * 4) as usize;
                        if idx + 3 < buffer.len() {
                            buffer[idx + 0] = TEXT_COLOR.0;
                            buffer[idx + 1] = TEXT_COLOR.1;
                            buffer[idx + 2] = TEXT_COLOR.2;
                            buffer[idx + 3] = val;
                        }
                    }
                }
            }
        }
    }
}

