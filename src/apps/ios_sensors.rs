use crate::engine::{Application, EngineState};
use crate::sensors::{Magnetometer, MagnetometerReading};
use crate::text::text_rasterization::TextRasterizer;
use crate::ui::Selector;
use fontdue::{Font, FontSettings};
use std::time::{Instant, Duration};

const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);
const TEXT_COLOR: (u8, u8, u8) = (255, 255, 255);
const BUTTON_COLOR: (u8, u8, u8) = (60, 60, 60);
const BUTTON_HOVER_COLOR: (u8, u8, u8) = (80, 80, 80);
const BUTTON_BORDER_COLOR: (u8, u8, u8) = (120, 120, 120);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SensorType {
    Magnetometer,
    Accelerometer,
    Rotation,
    Gyroscope,
    Barometer,
}

impl SensorType {
    fn name(&self) -> &'static str {
        match self {
            SensorType::Magnetometer => "Magnetometer",
            SensorType::Accelerometer => "Accelerometer",
            SensorType::Rotation => "Rotation",
            SensorType::Gyroscope => "Gyroscope",
            SensorType::Barometer => "Barometer",
        }
    }
    
    fn from_index(idx: usize) -> Self {
        match idx {
            0 => SensorType::Magnetometer,
            1 => SensorType::Accelerometer,
            2 => SensorType::Rotation,
            3 => SensorType::Gyroscope,
            4 => SensorType::Barometer,
            _ => SensorType::Magnetometer,
        }
    }
}

pub struct IosSensorsApp {
    magnetometer: Option<Magnetometer>,
    magnitude_text: TextRasterizer,
    coordinates_text: TextRasterizer,
    count_text: TextRasterizer,
    rate_text: TextRasterizer,
    button_text: TextRasterizer,
    last_reading: Option<MagnetometerReading>,
    // For calculating readings per second
    last_rate_calc_time: Instant,
    readings_since_last_calc: u64,
    readings_per_second: f64,
    // Sensor selection
    current_sensor: SensorType,
    sensor_selector: Selector,
    // Button state
    button_hovered: bool,
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
            24.0 * 1.2 // 20% larger
        } else {
            20.0 * 1.2 // 20% larger
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
        let button_font = Font::from_bytes(font_bytes, FontSettings::default())
            .expect("Failed to load font");

        let magnitude_text = TextRasterizer::new(magnitude_font, magnitude_font_size);
        let coordinates_text = TextRasterizer::new(coordinates_font, coordinates_font_size);
        let count_text = TextRasterizer::new(count_font, small_font_size);
        let rate_text = TextRasterizer::new(rate_font, small_font_size);
        let button_text = TextRasterizer::new(button_font, small_font_size);

        // Create sensor selector with all sensor options
        let sensor_options = vec![
            "Magnetometer".to_string(),
            "Accelerometer".to_string(),
            "Rotation".to_string(),
            "Gyroscope".to_string(),
            "Barometer".to_string(),
        ];
        let sensor_selector = Selector::new(sensor_options);

        Self {
            magnetometer: None,
            magnitude_text,
            coordinates_text,
            count_text,
            rate_text,
            button_text,
            last_reading: None,
            last_rate_calc_time: Instant::now(),
            readings_since_last_calc: 0,
            readings_per_second: 0.0,
            current_sensor: SensorType::Magnetometer,
            sensor_selector,
            button_hovered: false,
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
        if self.current_sensor != SensorType::Magnetometer {
            // Show "no reads" for non-magnetometer sensors
            self.magnitude_text.set_text("no reads".to_string());
            self.coordinates_text.set_text("no reads".to_string());
        } else if let Some(reading) = self.last_reading {
            let magnitude = reading.magnitude();
            self.magnitude_text.set_text(format!("{:.4}μT", magnitude));
            
            // Update coordinates text (X, Y, Z on same line, 1 decimal each)
            self.coordinates_text.set_text(format!(
                "X: {:.1}μT, Y: {:.1}μT, Z: {:.1}μT",
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

        // Update button text with current sensor name
        self.button_text.set_text(self.current_sensor.name().to_string());
        self.button_text.tick(width as f32, height as f32);

        // Update selector
        self.sensor_selector.update(width as f32, height as f32);

        // Check if selector is closed and we have a selection
        if !self.sensor_selector.is_open() {
            if let Some(selected_idx) = self.sensor_selector.selected() {
                let new_sensor = SensorType::from_index(selected_idx);
                if new_sensor != self.current_sensor {
                    self.current_sensor = new_sensor;
                    // TODO: Initialize the new sensor when we implement them
                }
            }
        }

        // Calculate positions for centering - vertically centered layout
        let center_y = height as f32 / 2.0;
        let line_spacing = 50.0;
        
        // Button position (above text)
        let button_height = 84.5;
        let button_padding = 20.0;
        let button_y_pos = center_y - line_spacing * 1.5 - button_height - button_padding;
        
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

        // Draw sensor selector button (above text)
        self.draw_sensor_button(buffer, width, height, button_y_pos);
        
        // Draw all text
        self.draw_text_geometric(buffer, width, height, &self.magnitude_text, magnitude_x, magnitude_y);
        self.draw_text_geometric(buffer, width, height, &self.coordinates_text, coordinates_x, coordinates_y);
        self.draw_text_geometric(buffer, width, height, &self.count_text, count_x, count_y);
        self.draw_text_geometric(buffer, width, height, &self.rate_text, rate_x, rate_y);
        
        // Render selector if open
        if self.sensor_selector.is_open() {
            self.sensor_selector.render(state);
        }
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        // Check if selector is open - handle selector clicks first
        if self.sensor_selector.is_open() {
            let shape = state.frame.array.shape();
            let width = shape[1] as f32;
            let height = shape[0] as f32;
            let mouse_x = state.mouse.x;
            let mouse_y = state.mouse.y;
            
            // Check if click was handled by selector (clicked on an option)
            if self.sensor_selector.on_mouse_down(state) {
                return;
            }
            
            // If click was outside selector, close it (default is magnetometer so always allow closing)
            if self.sensor_selector.is_click_outside(mouse_x, mouse_y, width, height) {
                self.sensor_selector.close();
            }
            return;
        }
        
        // Check if button was clicked
        let shape = state.frame.array.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        let center_y = height / 2.0;
        let line_spacing = 50.0;
        let button_height = 84.5; // 30% bigger than 65.0
        let button_padding = 20.0;
        let button_y = center_y - line_spacing * 1.5 - button_height - button_padding;
        let button_width = 338.0; // 30% bigger than 260.0
        let button_x = (width - button_width) / 2.0;
        
        let mouse_x = state.mouse.x;
        let mouse_y = state.mouse.y;
        
        if mouse_x >= button_x && mouse_x <= button_x + button_width &&
           mouse_y >= button_y && mouse_y <= button_y + button_height {
            // Button clicked - open selector
            self.sensor_selector.open();
        }
    }
    
    fn on_mouse_up(&mut self, _state: &mut EngineState) {
        // Nothing needed here for now
    }
    
    fn on_mouse_move(&mut self, state: &mut EngineState) {
        // Update button hover state
        let shape = state.frame.array.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        let center_y = height / 2.0;
        let line_spacing = 50.0;
        let button_height = 84.5; // 30% bigger than 65.0
        let button_padding = 20.0;
        let button_y = center_y - line_spacing * 1.5 - button_height - button_padding;
        let button_width = 338.0; // 30% bigger than 260.0
        let button_x = (width - button_width) / 2.0;
        
        let mouse_x = state.mouse.x;
        let mouse_y = state.mouse.y;
        
        self.button_hovered = mouse_x >= button_x && mouse_x <= button_x + button_width &&
                              mouse_y >= button_y && mouse_y <= button_y + button_height;
    }
}

impl IosSensorsApp {
    fn draw_text_geometric(&self, buffer: &mut [u8], width: u32, height: u32, text_rasterizer: &TextRasterizer, offset_x: f32, offset_y: f32) {
        for character in &text_rasterizer.characters {
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
    
    fn draw_sensor_button(&self, buffer: &mut [u8], width: u32, height: u32, button_y_pos: f32) {
        let button_y = button_y_pos as i32;
        let button_height = 85; // 30% bigger than 65 (rounded)
        let button_width = 338; // 30% bigger than 260
        let button_x = ((width as f32 - button_width as f32) / 2.0) as i32;
        
        // Choose button color based on hover state
        let button_color = if self.button_hovered {
            BUTTON_HOVER_COLOR
        } else {
            BUTTON_COLOR
        };
        
        // Draw button background (rounded rectangle - simplified as rectangle)
        for py in button_y..(button_y + button_height) {
            for px in button_x..(button_x + button_width) {
                if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                    let idx = ((py as u32 * width + px as u32) * 4) as usize;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = button_color.0;
                        buffer[idx + 1] = button_color.1;
                        buffer[idx + 2] = button_color.2;
                        buffer[idx + 3] = 255;
                    }
                }
            }
        }
        
        // Draw button border
        let border_thickness = 2;
        for t in 0..border_thickness {
            // Top border
            for px in button_x..(button_x + button_width) {
                let py = button_y + t;
                if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                    let idx = ((py as u32 * width + px as u32) * 4) as usize;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = BUTTON_BORDER_COLOR.0;
                        buffer[idx + 1] = BUTTON_BORDER_COLOR.1;
                        buffer[idx + 2] = BUTTON_BORDER_COLOR.2;
                        buffer[idx + 3] = 255;
                    }
                }
            }
            // Bottom border
            for px in button_x..(button_x + button_width) {
                let py = button_y + button_height - 1 - t;
                if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                    let idx = ((py as u32 * width + px as u32) * 4) as usize;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = BUTTON_BORDER_COLOR.0;
                        buffer[idx + 1] = BUTTON_BORDER_COLOR.1;
                        buffer[idx + 2] = BUTTON_BORDER_COLOR.2;
                        buffer[idx + 3] = 255;
                    }
                }
            }
            // Left border
            for py in button_y..(button_y + button_height) {
                let px = button_x + t;
                if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                    let idx = ((py as u32 * width + px as u32) * 4) as usize;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = BUTTON_BORDER_COLOR.0;
                        buffer[idx + 1] = BUTTON_BORDER_COLOR.1;
                        buffer[idx + 2] = BUTTON_BORDER_COLOR.2;
                        buffer[idx + 3] = 255;
                    }
                }
            }
            // Right border
            for py in button_y..(button_y + button_height) {
                let px = button_x + button_width - 1 - t;
                if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                    let idx = ((py as u32 * width + px as u32) * 4) as usize;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = BUTTON_BORDER_COLOR.0;
                        buffer[idx + 1] = BUTTON_BORDER_COLOR.1;
                        buffer[idx + 2] = BUTTON_BORDER_COLOR.2;
                        buffer[idx + 3] = 255;
                    }
                }
            }
        }
        
        // Draw button text (bottom-aligned, horizontally centered)
        let button_text_width = if let Some(last_char) = self.button_text.characters.last() {
            last_char.x + last_char.metrics.advance_width
        } else {
            0.0
        };
        let button_text_x = button_x as f32 + (button_width as f32 - button_text_width) / 2.0;
        
        // Bottom-align text vertically in the button
        // The text baseline is at ascent from the top of the text area
        // We want the bottom of the text (baseline + descent) to be at button_bottom - padding
        let button_text_bottom_padding = 10.0; // Padding from bottom
        let button_bottom_y = button_y as f32 + button_height as f32;
        // Calculate baseline position: bottom - padding - descent
        let baseline_y = button_bottom_y - button_text_bottom_padding - self.button_text.descent;
        // The top of the text area (where character.y=0) is at baseline_y - ascent
        let button_text_y = baseline_y - self.button_text.ascent;
        
        self.draw_text_geometric(buffer, width, height, &self.button_text, button_text_x, button_text_y);
    }
}

