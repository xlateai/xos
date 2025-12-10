use crate::engine::EngineState;
use crate::apps::text::geometric::GeometricText;
use crate::text::fonts;

/// A simple, reusable selector component for choosing between options
pub struct Selector {
    /// Whether the selector is currently visible/open
    is_open: bool,
    /// The currently selected option index (None if nothing selected)
    selected: Option<usize>,
    /// The options to choose from
    options: Vec<String>,
    /// Animation progress (0.0 = closed, 1.0 = fully open)
    animation_progress: f32,
    /// Animation speed (how fast it opens/closes)
    animation_speed: f32,
    /// Text renderers for each option
    text_renderers: Vec<GeometricText>,
    /// Font size for option text
    font_size: f32,
}

impl Selector {
    /// Create a new selector with the given options
    pub fn new(options: Vec<String>) -> Self {
        let font_size = 24.0;
        
        // Create a text renderer for each option
        // Each renderer gets its own font instance (fontdue Font is not Clone)
        let text_renderers: Vec<GeometricText> = options.iter()
            .map(|option| {
                let font = fonts::jetbrains_mono();
                let mut renderer = GeometricText::new(font, font_size);
                renderer.set_text(option.clone());
                renderer
            })
            .collect();

        Self {
            is_open: false,
            selected: None,
            options,
            animation_progress: 0.0,
            animation_speed: 0.15, // Smooth animation speed
            text_renderers,
            font_size,
        }
    }

    /// Open the selector
    pub fn open(&mut self) {
        self.is_open = true;
    }

    /// Close the selector
    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Toggle the selector open/closed
    pub fn toggle(&mut self) {
        self.is_open = !self.is_open;
    }

    /// Check if the selector is open
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// Get the currently selected option index
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Get the selected option as a string
    pub fn selected_option(&self) -> Option<&str> {
        self.selected.and_then(|idx| self.options.get(idx)).map(|s| s.as_str())
    }

    /// Get the selected option index
    pub fn selected_index(&self) -> Option<usize> {
        self.selected
    }

    /// Handle mouse down event - returns true if the click was handled
    pub fn on_mouse_down(&mut self, state: &mut EngineState) -> bool {
        if !self.is_open {
            return false;
        }

        let mouse_x = state.mouse.x;
        let mouse_y = state.mouse.y;
        let width = state.frame.width as f32;
        let height = state.frame.height as f32;

        // Calculate selector position (centered)
        let selector_width = 300.0;
        let selector_height = (self.options.len() as f32 * 50.0) + 40.0; // 50px per option + padding
        let x = (width - selector_width) / 2.0;
        let y = (height - selector_height) / 2.0;

        // Check if click is within selector bounds
        if mouse_x >= x && mouse_x <= x + selector_width &&
           mouse_y >= y && mouse_y <= y + selector_height {
            // Check which option was clicked
            let option_height = 50.0;
            let start_y = y + 20.0; // Top padding
            let click_y = mouse_y - start_y;
            
            if click_y >= 0.0 {
                let option_idx = (click_y / option_height) as usize;
                if option_idx < self.options.len() {
                    self.selected = Some(option_idx);
                    self.close();
                    return true;
                }
            }
        }

        false
    }

    /// Update the selector (handles animation and text rendering)
    pub fn update(&mut self, width: f32, height: f32) {
        if self.is_open {
            // Animate opening
            self.animation_progress = (self.animation_progress + self.animation_speed).min(1.0);
        } else {
            // Animate closing
            self.animation_progress = (self.animation_progress - self.animation_speed).max(0.0);
        }

        // Update text renderers with a wide enough width so text doesn't wrap
        // Use a large width to ensure single-line rendering
        for text_renderer in &mut self.text_renderers {
            text_renderer.tick(10000.0, height); // Large width to prevent wrapping
        }
    }

    /// Render the selector to the frame buffer
    pub fn render(&self, state: &mut EngineState) {
        if self.animation_progress <= 0.0 {
            return; // Fully closed, don't render
        }

        let buffer = &mut state.frame.buffer;
        let width = state.frame.width;
        let height = state.frame.height;

        // Calculate selector dimensions and position
        let selector_width = 300.0;
        let option_height = 50.0;
        let selector_height = (self.options.len() as f32 * option_height) + 40.0;
        let center_x = width as f32 / 2.0;
        let center_y = height as f32 / 2.0;
        let x = (center_x - selector_width / 2.0) as i32;
        let y = (center_y - selector_height / 2.0) as i32;

        // Apply animation (fade + slight scale)
        let alpha = self.animation_progress;
        let scale = 0.9 + (self.animation_progress * 0.1); // Scale from 0.9 to 1.0

        // Draw semi-transparent background overlay
        let overlay_alpha = (alpha * 180.0) as u8;
        for py in 0..height {
            for px in 0..width {
                let idx = ((py * width + px) * 4) as usize;
                if idx + 3 < buffer.len() {
                    // Darken the background
                    buffer[idx + 0] = (buffer[idx + 0] as u16 * (255 - overlay_alpha as u16) / 255) as u8;
                    buffer[idx + 1] = (buffer[idx + 1] as u16 * (255 - overlay_alpha as u16) / 255) as u8;
                    buffer[idx + 2] = (buffer[idx + 2] as u16 * (255 - overlay_alpha as u16) / 255) as u8;
                }
            }
        }

        // Draw selector box with animation
        let scaled_width = (selector_width * scale) as i32;
        let scaled_height = (selector_height * scale) as i32;
        let scaled_x = (center_x - scaled_width as f32 / 2.0) as i32;
        let scaled_y = (center_y - scaled_height as f32 / 2.0) as i32;

        // Background color (dark gray with transparency)
        let bg_r = 45;
        let bg_g = 45;
        let bg_b = 45;
        let bg_alpha = (alpha * 255.0) as u8;

        // Draw rounded rectangle background (simplified - just a rectangle for now)
        for py in scaled_y..(scaled_y + scaled_height) {
            for px in scaled_x..(scaled_x + scaled_width) {
                if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                    let idx = ((py as u32 * width + px as u32) * 4) as usize;
                    if idx + 3 < buffer.len() {
                        // Blend with background
                        let blend_alpha = bg_alpha as f32 / 255.0;
                        buffer[idx + 0] = ((bg_r as f32 * blend_alpha) + (buffer[idx + 0] as f32 * (1.0 - blend_alpha))) as u8;
                        buffer[idx + 1] = ((bg_g as f32 * blend_alpha) + (buffer[idx + 1] as f32 * (1.0 - blend_alpha))) as u8;
                        buffer[idx + 2] = ((bg_b as f32 * blend_alpha) + (buffer[idx + 2] as f32 * (1.0 - blend_alpha))) as u8;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
        }

        // Draw border
        let border_color = (200, 200, 200);
        let border_alpha = (alpha * 255.0) as u8;
        let border_thickness = 2;

        // Top and bottom borders
        for px in scaled_x..(scaled_x + scaled_width) {
            for t in 0..border_thickness {
                // Top border
                let py = scaled_y + t;
                if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                    let idx = ((py as u32 * width + px as u32) * 4) as usize;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = border_color.0;
                        buffer[idx + 1] = border_color.1;
                        buffer[idx + 2] = border_color.2;
                        buffer[idx + 3] = border_alpha;
                    }
                }
                // Bottom border
                let py = scaled_y + scaled_height - 1 - t;
                if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                    let idx = ((py as u32 * width + px as u32) * 4) as usize;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = border_color.0;
                        buffer[idx + 1] = border_color.1;
                        buffer[idx + 2] = border_color.2;
                        buffer[idx + 3] = border_alpha;
                    }
                }
            }
        }

        // Left and right borders
        for py in scaled_y..(scaled_y + scaled_height) {
            for t in 0..border_thickness {
                // Left border
                let px = scaled_x + t;
                if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                    let idx = ((py as u32 * width + px as u32) * 4) as usize;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = border_color.0;
                        buffer[idx + 1] = border_color.1;
                        buffer[idx + 2] = border_color.2;
                        buffer[idx + 3] = border_alpha;
                    }
                }
                // Right border
                let px = scaled_x + scaled_width - 1 - t;
                if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                    let idx = ((py as u32 * width + px as u32) * 4) as usize;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = border_color.0;
                        buffer[idx + 1] = border_color.1;
                        buffer[idx + 2] = border_color.2;
                        buffer[idx + 3] = border_alpha;
                    }
                }
            }
        }

        // Draw options with text
        let option_start_y = scaled_y + 20;
        let option_width = scaled_width - 40;
        let mouse_x = state.mouse.x as i32;
        let mouse_y = state.mouse.y as i32;

        // Text color (bright white for visibility)
        let text_color = (255, 255, 255);

        for (idx, option) in self.options.iter().enumerate() {
            let option_y = option_start_y + (idx as i32 * option_height as i32);
            let option_rect_y = option_y;
            let option_rect_height = option_height as i32 - 4;

            // Check if mouse is hovering over this option
            let is_hovered = mouse_x >= scaled_x + 20 && mouse_x <= scaled_x + 20 + option_width &&
                            mouse_y >= option_rect_y && mouse_y <= option_rect_y + option_rect_height;

            // Draw option background (highlight if hovered)
            let option_bg = if is_hovered {
                (70, 70, 90) // Slightly lighter when hovered
            } else {
                (55, 55, 55) // Default option background
            };

            for py in option_rect_y..(option_rect_y + option_rect_height) {
                for px in (scaled_x + 20)..(scaled_x + 20 + option_width) {
                    if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                        let idx = ((py as u32 * width + px as u32) * 4) as usize;
                        if idx + 3 < buffer.len() {
                            let blend_alpha = (alpha * 0.8) as f32;
                            buffer[idx + 0] = ((option_bg.0 as f32 * blend_alpha) + (buffer[idx + 0] as f32 * (1.0 - blend_alpha))) as u8;
                            buffer[idx + 1] = ((option_bg.1 as f32 * blend_alpha) + (buffer[idx + 1] as f32 * (1.0 - blend_alpha))) as u8;
                            buffer[idx + 2] = ((option_bg.2 as f32 * blend_alpha) + (buffer[idx + 2] as f32 * (1.0 - blend_alpha))) as u8;
                            buffer[idx + 3] = 0xff;
                        }
                    }
                }
            }

            // Render text for this option
            if let Some(text_renderer) = self.text_renderers.get(idx) {
                // Calculate text position (centered vertically in option, left-aligned with padding)
                let text_x = (scaled_x + 20 + 10) as f32; // Left padding
                
                // Center text vertically in the option box
                // The renderer's coordinate system: y=0 is top, y=ascent is first baseline
                // character.y is the top of the character bitmap
                let option_center_y = option_rect_y as f32 + option_rect_height as f32 / 2.0;
                // We want the text baseline at the center, so offset by ascent
                let text_top = option_center_y - text_renderer.ascent;

                // Draw each character
                for character in &text_renderer.characters {
                    let px = (text_x + character.x) as i32;
                    // character.y is relative to y=0 (top of text area in renderer's coords)
                    // Add text_top to position it in the option box
                    let py = (text_top + character.y) as i32;

                    // Draw character bitmap (similar to text.rs)
                    for y in 0..character.metrics.height {
                        for x in 0..character.metrics.width {
                            let val = character.bitmap[y * character.metrics.width + x];
                            if val > 0 {
                                let sx = px + x as i32;
                                let sy = py + y as i32;

                                if sx >= 0 && sx < width as i32 && sy >= 0 && sy < height as i32 {
                                    let idx = ((sy as u32 * width + sx as u32) * 4) as usize;
                                    if idx + 3 < buffer.len() {
                                        // Draw text directly (overwrite background)
                                        // Apply alpha from both the bitmap value and animation
                                        let bitmap_alpha = val as f32 / 255.0;
                                        let final_alpha = (bitmap_alpha * alpha) as f32;
                                        
                                        // Blend text color with background
                                        let bg_r = buffer[idx + 0] as f32;
                                        let bg_g = buffer[idx + 1] as f32;
                                        let bg_b = buffer[idx + 2] as f32;
                                        
                                        buffer[idx + 0] = ((text_color.0 as f32 * final_alpha) + (bg_r * (1.0 - final_alpha))) as u8;
                                        buffer[idx + 1] = ((text_color.1 as f32 * final_alpha) + (bg_g * (1.0 - final_alpha))) as u8;
                                        buffer[idx + 2] = ((text_color.2 as f32 * final_alpha) + (bg_b * (1.0 - final_alpha))) as u8;
                                        buffer[idx + 3] = 0xff;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
