use crate::engine::{Application, EngineState};
use crate::sensors::Magnetometer;
use crate::apps::text::geometric::GeometricText;
use fontdue::{Font, FontSettings};

const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);
const TEXT_COLOR: (u8, u8, u8) = (255, 255, 255);

pub struct IosSensorsApp {
    magnetometer: Option<Magnetometer>,
    reading_text: GeometricText,
    count_text: GeometricText,
}

impl IosSensorsApp {
    pub fn new() -> Self {
        let font_bytes = include_bytes!("../../assets/JetBrainsMono-Regular.ttf") as &[u8];
        
        // Large font size for displaying numbers
        let large_font_size = if cfg!(target_os = "ios") {
            120.0
        } else {
            80.0
        };
        
        // Smaller font size for count
        let small_font_size = large_font_size * 0.4;

        // Load font twice since Font doesn't implement Clone
        let reading_font = Font::from_bytes(font_bytes, FontSettings::default())
            .expect("Failed to load font");
        let count_font = Font::from_bytes(font_bytes, FontSettings::default())
            .expect("Failed to load font");

        let reading_text = GeometricText::new(reading_font, large_font_size);
        let count_text = GeometricText::new(count_font, small_font_size);

        Self {
            magnetometer: None,
            reading_text,
            count_text,
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
        let (latest_reading, total_readings) = if let Some(mag) = &mut self.magnetometer {
            // Wrap in catch_unwind in case drain panics
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let batch = mag.drain_readings();
                let latest = batch.last().copied();
                let total = mag.get_total_readings();
                (latest, total)
            })) {
                Ok((r, t)) => (r, t),
                Err(_) => {
                    // If accessing magnetometer panics, treat as no reading
                    (None, 0)
                }
            }
        } else {
            (None, 0)
        };

        // Calculate magnitude if we have a reading
        let magnitude = latest_reading.map(|reading| reading.magnitude());

        // Update reading text
        let reading_text_str = if let Some(mag) = magnitude {
            format!("{:.2}uT", mag)
        } else {
            "No reading".to_string()
        };
        self.reading_text.set_text(reading_text_str);
        self.reading_text.tick(width as f32, height as f32);

        // Update count text
        let count_text_str = format!("{} readings", total_readings);
        self.count_text.set_text(count_text_str);
        self.count_text.tick(width as f32, height as f32);

        // Calculate positions for centering
        // Use advance_width (not bitmap width) for accurate text width calculation
        let reading_text_width = if let Some(last_char) = self.reading_text.characters.last() {
            last_char.x + last_char.metrics.advance_width
        } else {
            0.0
        };
        let reading_x = (width as f32 - reading_text_width) / 2.0;
        let reading_y = height as f32 / 2.0 - 60.0;

        let count_text_width = if let Some(last_char) = self.count_text.characters.last() {
            last_char.x + last_char.metrics.advance_width
        } else {
            0.0
        };
        let count_x = (width as f32 - count_text_width) / 2.0;
        let count_y = height as f32 / 2.0 + 80.0;

        // Draw reading text
        self.draw_text_geometric(buffer, width, height, &self.reading_text, reading_x, reading_y);
        
        // Draw count text
        self.draw_text_geometric(buffer, width, height, &self.count_text, count_x, count_y);
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

