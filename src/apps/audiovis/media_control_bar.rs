use crate::engine::EngineState;

/// Media control bar with play/pause button and seek bar
pub struct MediaControlBar {
    /// Whether playback is currently paused
    is_paused: bool,
    /// Current playback position (0.0 to 1.0)
    position: f32,
    /// Whether the user is currently dragging the seek handle
    is_dragging: bool,
    /// Size of the play/pause button
    button_size: f32,
    /// Radius of the seek handle circle
    handle_radius: f32,
    /// Last position that triggered a visualization update
    last_update_position: f32,
}

impl MediaControlBar {
    pub fn new() -> Self {
        Self {
            is_paused: false,
            position: 0.0,
            is_dragging: false,
            button_size: 60.0, // Larger button
            handle_radius: 10.0, // Larger handle
            last_update_position: -1.0,
        }
    }

    /// Get the current playback position (0.0 to 1.0)
    pub fn position(&self) -> f32 {
        self.position
    }

    /// Check if playback is paused
    pub fn is_paused(&self) -> bool {
        self.is_paused
    }

    /// Check if currently dragging the seek handle
    pub fn is_dragging(&self) -> bool {
        self.is_dragging
    }

    /// Set paused state
    pub fn set_paused(&mut self, paused: bool) {
        self.is_paused = paused;
    }

    /// Set position (for external updates from audio)
    pub fn set_position(&mut self, position: f32) {
        if !self.is_dragging {
            self.position = position.max(0.0).min(1.0);
        }
    }

    /// Check if position changed significantly (for forcing visualization update)
    pub fn position_changed(&mut self) -> bool {
        let changed = (self.position - self.last_update_position).abs() > 0.001;
        if changed {
            self.last_update_position = self.position;
        }
        changed
    }

    /// Update the control bar (handles auto-advance when playing)
    pub fn update(&mut self, state: &mut EngineState) {
        // Handle dragging
        if self.is_dragging && state.mouse.is_left_clicking {
            self.update_seek_position(state);
        } else if !state.mouse.is_left_clicking {
            self.is_dragging = false;
        }
    }

    /// Update seek position from mouse (called during drag)
    pub fn update_seek_from_mouse(&mut self, state: &mut EngineState) {
        if self.is_dragging {
            self.update_seek_position(state);
        }
    }

    /// Handle mouse down event
    pub fn on_mouse_down(&mut self, state: &mut EngineState) -> bool {
        let mouse_x = state.mouse.x;
        let mouse_y = state.mouse.y;
        let width = state.frame.width as f32;
        let height = state.frame.height as f32;

        // Button is centered horizontally, slightly up from center
        let button_center_x = width / 2.0;
        let button_center_y = height / 2.0 - 40.0; // Slightly up from center
        let button_dist = ((mouse_x - button_center_x).powi(2) + (mouse_y - button_center_y).powi(2)).sqrt();
        
        if button_dist <= self.button_size / 2.0 {
            self.is_paused = !self.is_paused;
            return true;
        }

        // Seek bar is horizontal line across the screen (no background)
        let seek_y = height / 2.0 - 40.0; // Same Y as button
        let seek_tolerance = 20.0; // Click tolerance around the line
        let seek_x_start = 50.0; // Padding from edges
        let seek_x_end = width - 50.0;

        // Check if click is near the seek line
        if (mouse_y - seek_y).abs() < seek_tolerance && mouse_x >= seek_x_start && mouse_x <= seek_x_end {
            // Start dragging
            self.is_dragging = true;
            self.update_seek_position(state);
            return true;
        }

        false
    }

    /// Update seek position based on mouse position
    fn update_seek_position(&mut self, state: &mut EngineState) {
        let mouse_x = state.mouse.x;
        let width = state.frame.width as f32;

        let seek_x_start = 50.0;
        let seek_x_end = width - 50.0;
        let seek_width = seek_x_end - seek_x_start;

        // Calculate position based on mouse X
        let relative_x = (mouse_x - seek_x_start).max(0.0).min(seek_width);
        self.position = (relative_x / seek_width).max(0.0).min(1.0);
        // Force visualization update on seek
        self.last_update_position = -1.0;
    }

    /// Render the control bar
    pub fn render(&self, state: &mut EngineState) {
        let buffer = &mut state.frame.buffer;
        let width = state.frame.width;
        let height = state.frame.height;

        let button_center_x = (width as f32 / 2.0) as i32;
        let button_center_y = ((height as f32 / 2.0) - 40.0) as i32;
        let button_radius = (self.button_size / 2.0) as i32;

        // Draw play/pause button (larger, centered)
        let button_bg = (80, 80, 80); // Dark gray circle
        self.draw_circle(buffer, width, height, button_center_x, button_center_y, button_radius, button_bg);

        // Draw play or pause icon
        let icon_color = (255, 255, 255);
        if self.is_paused {
            // Draw play triangle (pointing right)
            let size = 18; // Larger icon
            let tx = button_center_x;
            let ty = button_center_y;
            
            // Draw a right-pointing triangle
            for dy in -size..=size {
                for dx in -size..=size {
                    let px = tx + dx;
                    let py = ty + dy;
                    
                    // Triangle bounds: x from -size/2 to size/2, y from -size/2 to size/2
                    // For each y, x ranges from -size/2 to size/2 - abs(y)
                    let in_triangle = dy >= -size / 2 && dy <= size / 2 &&
                                     dx >= -size / 2 && dx <= (size / 2 - dy.abs());
                    
                    if in_triangle && px >= 0 && py >= 0 && (px as u32) < width && (py as u32) < height {
                        let idx = ((py as u32 * width + px as u32) * 4) as usize;
                        if idx + 3 < buffer.len() {
                            buffer[idx + 0] = icon_color.0;
                            buffer[idx + 1] = icon_color.1;
                            buffer[idx + 2] = icon_color.2;
                            buffer[idx + 3] = 0xff;
                        }
                    }
                }
            }
        } else {
            // Draw pause icon (two vertical bars)
            let bar_width = 5;
            let bar_height = 20;
            let bar_spacing = 8;
            
            // Left bar
            let left_bar_x = button_center_x - bar_spacing / 2 - bar_width;
            for py in (button_center_y - bar_height / 2)..(button_center_y + bar_height / 2) {
                for px in left_bar_x..(left_bar_x + bar_width) {
                    if px >= 0 && py >= 0 && (px as u32) < width && (py as u32) < height {
                        let idx = ((py as u32 * width + px as u32) * 4) as usize;
                        if idx + 3 < buffer.len() {
                            buffer[idx + 0] = icon_color.0;
                            buffer[idx + 1] = icon_color.1;
                            buffer[idx + 2] = icon_color.2;
                            buffer[idx + 3] = 0xff;
                        }
                    }
                }
            }
            
            // Right bar
            let right_bar_x = button_center_x + bar_spacing / 2;
            for py in (button_center_y - bar_height / 2)..(button_center_y + bar_height / 2) {
                for px in right_bar_x..(right_bar_x + bar_width) {
                    if px >= 0 && py >= 0 && (px as u32) < width && (py as u32) < height {
                        let idx = ((py as u32 * width + px as u32) * 4) as usize;
                        if idx + 3 < buffer.len() {
                            buffer[idx + 0] = icon_color.0;
                            buffer[idx + 1] = icon_color.1;
                            buffer[idx + 2] = icon_color.2;
                            buffer[idx + 3] = 0xff;
                        }
                    }
                }
            }
        }

        // Draw seek bar (plain white line, no background)
        let seek_y = button_center_y;
        let seek_x_start = 50;
        let seek_x_end = (width as f32 - 50.0) as i32;
        let seek_width = seek_x_end - seek_x_start;
        let line_height = 2; // Thin white line

        // Draw white line
        let line_color = (255, 255, 255);
        for py in (seek_y - line_height / 2)..(seek_y + line_height / 2 + 1) {
            for px in seek_x_start..seek_x_end {
                if px >= 0 && py >= 0 && (px as u32) < width && (py as u32) < height {
                    let idx = ((py as u32 * width + px as u32) * 4) as usize;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = line_color.0;
                        buffer[idx + 1] = line_color.1;
                        buffer[idx + 2] = line_color.2;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
        }

        // Draw seek handle (silver circle, raised up)
        let handle_x = seek_x_start + (seek_width as f32 * self.position) as i32;
        let handle_y = seek_y - 5; // Raised up 5 pixels
        let handle_color = (192, 192, 192); // Silver color
        let handle_radius_i = self.handle_radius as i32;
        
        // Draw handle with slight shadow/outline for raised effect
        let outline_color = (150, 150, 150);
        self.draw_circle(buffer, width, height, handle_x, handle_y, handle_radius_i + 1, outline_color);
        self.draw_circle(buffer, width, height, handle_x, handle_y, handle_radius_i, handle_color);
    }

    /// Draw a filled circle
    fn draw_circle(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        center_x: i32,
        center_y: i32,
        radius: i32,
        color: (u8, u8, u8),
    ) {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let dist_sq = dx * dx + dy * dy;
                if dist_sq <= radius * radius {
                    let px = center_x + dx;
                    let py = center_y + dy;
                    if px >= 0 && py >= 0 && (px as u32) < width && (py as u32) < height {
                        let idx = ((py as u32 * width + px as u32) * 4) as usize;
                        if idx + 3 < buffer.len() {
                            buffer[idx + 0] = color.0;
                            buffer[idx + 1] = color.1;
                            buffer[idx + 2] = color.2;
                            buffer[idx + 3] = 0xff;
                        }
                    }
                }
            }
        }
    }
}
