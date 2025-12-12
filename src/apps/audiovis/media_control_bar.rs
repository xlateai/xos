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
    /// Whether we just seeked (to prevent position from being reset)
    just_seeked: bool,
    /// Frames since last seek (to keep just_seeked active for multiple frames)
    frames_since_seek: u32,
}

impl MediaControlBar {
    pub fn new() -> Self {
        Self {
            is_paused: false,
            position: 0.0,
            is_dragging: false,
            button_size: 84.0, // 20% bigger (70 * 1.2)
            handle_radius: 12.0,
            last_update_position: -1.0,
            just_seeked: false,
            frames_since_seek: 0,
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

    /// Check if position updates should be allowed (false if recently seeked or dragging)
    pub fn allow_position_update(&self) -> bool {
        !self.is_dragging && !self.just_seeked
    }

    /// Set paused state
    pub fn set_paused(&mut self, paused: bool) {
        self.is_paused = paused;
    }

    /// Set position (for external updates from audio)
    pub fn set_position(&mut self, position: f32) {
        // Only update if not dragging and not just seeked - this prevents position from being reset during/after seek
        // This is for automatic position updates from audio playback
        if !self.is_dragging && !self.just_seeked {
            self.position = position.max(0.0).min(1.0);
        }
    }

    /// Set position manually (for user seeking)
    pub fn set_position_manual(&mut self, position: f32) {
        // This is called when user manually seeks - always update and mark as just_seeked
        self.position = position.max(0.0).min(1.0);
        self.just_seeked = true;
        self.frames_since_seek = 0;
        self.last_update_position = -1.0; // Force visualization update
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
        // Track frames since seek - keep just_seeked active for several frames
        if self.just_seeked {
            self.frames_since_seek += 1;
            // Keep just_seeked true for 30 frames (about 0.5 second at 60fps)
            // This prevents position from being reset immediately after seeking
            // Longer duration ensures seeking works reliably
            if self.frames_since_seek > 30 {
                self.just_seeked = false;
                self.frames_since_seek = 0;
            }
        }
        
        // Handle dragging
        if self.is_dragging {
            if state.mouse.is_left_clicking {
                // Continue dragging
                self.update_seek_position(state);
            } else {
                // Mouse released, stop dragging but keep position
                self.is_dragging = false;
                // Position is already set via set_position_manual in update_seek_position
                // Just make sure just_seeked stays true
                if !self.just_seeked {
                    self.just_seeked = true;
                    self.frames_since_seek = 0;
                }
            }
        }
    }

    /// Update seek position from mouse (called during drag)
    pub fn update_seek_from_mouse(&mut self, state: &mut EngineState) {
        if self.is_dragging {
            self.update_seek_position(state);
        }
    }

    /// Calculate control bar layout (returns button center, seek bar bounds)
    fn calculate_layout(&self, width: f32, height: f32) -> (i32, i32, i32, i32, i32) {
        // Position at 10% from bottom
        let bottom_offset = height * 0.1;
        let control_y = (height - bottom_offset) as i32;
        
        // 80% width, centered
        let bar_width_pct = 0.8;
        let bar_width = (width * bar_width_pct) as i32;
        let bar_x_start = ((width - bar_width as f32) / 2.0) as i32;
        let bar_x_end = bar_x_start + bar_width;
        
        // Button is above the seek bar, centered horizontally
        let button_center_x = (width / 2.0) as i32;
        let button_center_y = control_y - 50; // 50px above the seek bar
        
        (button_center_x, button_center_y, bar_x_start, bar_x_end, control_y)
    }

    /// Handle mouse down event
    pub fn on_mouse_down(&mut self, state: &mut EngineState) -> bool {
        let mouse_x = state.mouse.x;
        let mouse_y = state.mouse.y;
        let shape = state.frame.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;

        let (button_center_x, button_center_y, seek_x_start, seek_x_end, seek_y) = 
            self.calculate_layout(width, height);

        // Check play/pause button
        let button_dist = ((mouse_x - button_center_x as f32).powi(2) + 
                          (mouse_y - button_center_y as f32).powi(2)).sqrt();
        
        if button_dist <= self.button_size / 2.0 {
            self.is_paused = !self.is_paused;
            return true;
        }

        // Check seek bar area (with tolerance)
        let seek_tolerance = 30.0; // Click tolerance around the line and handle
        if (mouse_y - seek_y as f32).abs() < seek_tolerance && 
           mouse_x >= seek_x_start as f32 && mouse_x <= seek_x_end as f32 {
            // Start dragging immediately and update position
            self.is_dragging = true;
            self.just_seeked = true; // Mark that we just seeked
            self.frames_since_seek = 0; // Reset counter
            self.update_seek_position(state);
            return true;
        }

        false
    }

    /// Update seek position based on mouse position
    fn update_seek_position(&mut self, state: &mut EngineState) {
        let mouse_x = state.mouse.x;
        let shape = state.frame.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;

        let (_, _, seek_x_start, seek_x_end, _) = self.calculate_layout(width, height);
        let seek_width = (seek_x_end - seek_x_start) as f32;

        // Calculate position based on mouse X
        let relative_x = (mouse_x - seek_x_start as f32).max(0.0).min(seek_width);
        let new_position = (relative_x / seek_width).max(0.0).min(1.0);
        
        // Use manual position setter to ensure it doesn't get overridden
        self.set_position_manual(new_position);
    }

    /// Render the control bar
    pub fn render(&self, state: &mut EngineState) {
        let shape = state.frame.shape();
        let width = shape[1] as u32;
        let height = shape[0] as u32;
        let buffer = state.frame_buffer_mut();

        let (button_center_x, button_center_y, seek_x_start, seek_x_end, seek_y) = 
            self.calculate_layout(width as f32, height as f32);
        let button_radius = (self.button_size / 2.0) as i32;

        // Draw play/pause button with crystal UI style (smooth gradient circle) - silver/white
        self.draw_crystal_circle(
            buffer, width, height, 
            button_center_x, button_center_y, 
            button_radius,
            (240, 240, 250), // Base color (super white silver)
            (220, 220, 230), // Shadow color (slightly darker silver)
        );

        // Draw play or pause icon - smooth and clean
        let icon_color = (255, 255, 255);
        if self.is_paused {
            // Draw play triangle (pointing right) - smooth
            let size = 26;
            let tx = button_center_x;
            let ty = button_center_y;
            
            // Draw smooth triangle
            for dy in -size..=size {
                for dx in -size..=size {
                    let px = tx + dx;
                    let py = ty + dy;
                    
                    // Triangle bounds
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
            // Draw pause icon (two vertical bars) - smooth
            let bar_width = 7;
            let bar_height = 29;
            let bar_spacing = 12;
            
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

        // Draw seek bar (crystal style - smooth white line with subtle glow)
        let seek_width = seek_x_end - seek_x_start;
        let line_height = 6;

        // Draw white line with subtle glow effect
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

        // Draw seek handle (crystal style - smooth silver circle with gradient)
        let handle_x = seek_x_start + (seek_width as f32 * self.position) as i32;
        let handle_y = seek_y;
        let handle_radius_i = self.handle_radius as i32;
        
        // Draw crystal-style handle with smooth anti-aliased circle
        self.draw_crystal_circle(
            buffer, width, height,
            handle_x, handle_y,
            handle_radius_i,
            (220, 220, 230), // Light silver
            (180, 180, 190), // Darker silver for shadow
        );
    }

    /// Draw a smooth anti-aliased circle with crystal UI style (gradient and shadow)
    fn draw_crystal_circle(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        center_x: i32,
        center_y: i32,
        radius: i32,
        base_color: (u8, u8, u8),
        shadow_color: (u8, u8, u8),
    ) {
        if radius <= 0 {
            return;
        }

        let radius_f = radius as f32;

        // Draw with anti-aliasing for smooth edges
        for dy in -radius - 2..=radius + 2 {
            for dx in -radius - 2..=radius + 2 {
                let px = center_x + dx;
                let py = center_y + dy;
                
                if px < 0 || py < 0 || (px as u32) >= width || (py as u32) >= height {
                    continue;
                }

                let dist_sq = (dx * dx + dy * dy) as f32;
                let dist = dist_sq.sqrt();
                
                if dist <= radius_f {
                    let idx = ((py as u32 * width + px as u32) * 4) as usize;
                    if idx + 3 >= buffer.len() {
                        continue;
                    }

                    // Calculate gradient based on distance from center
                    let normalized_dist = dist / radius_f;
                    
                    // Create gradient: lighter at top-left, darker at bottom-right
                    let gradient_factor = 1.0 - (normalized_dist * 0.3);
                    
                    // Add subtle shadow effect at bottom
                    let shadow_factor = if dy > 0 {
                        1.0 - (dy as f32 / radius_f) * 0.2
                    } else {
                        1.0
                    };

                    // Blend base and shadow colors
                    let r = (base_color.0 as f32 * gradient_factor * shadow_factor + 
                            shadow_color.0 as f32 * (1.0 - gradient_factor * shadow_factor)) as u8;
                    let g = (base_color.1 as f32 * gradient_factor * shadow_factor + 
                            shadow_color.1 as f32 * (1.0 - gradient_factor * shadow_factor)) as u8;
                    let b = (base_color.2 as f32 * gradient_factor * shadow_factor + 
                            shadow_color.2 as f32 * (1.0 - gradient_factor * shadow_factor)) as u8;

                    // Anti-aliasing at edges
                    let edge_dist = radius_f - dist;
                    let alpha = if edge_dist < 1.0 {
                        edge_dist.max(0.0).min(1.0)
                    } else {
                        1.0
                    };

                    // Blend with background
                    let bg_r = buffer[idx + 0] as f32;
                    let bg_g = buffer[idx + 1] as f32;
                    let bg_b = buffer[idx + 2] as f32;

                    buffer[idx + 0] = ((r as f32 * alpha) + (bg_r * (1.0 - alpha))) as u8;
                    buffer[idx + 1] = ((g as f32 * alpha) + (bg_g * (1.0 - alpha))) as u8;
                    buffer[idx + 2] = ((b as f32 * alpha) + (bg_b * (1.0 - alpha))) as u8;
                    buffer[idx + 3] = 0xff;
                } else if dist < radius_f + 1.0 {
                    // Anti-aliasing for edge pixels
                    let edge_dist = (radius_f + 1.0) - dist;
                    let alpha = edge_dist.max(0.0).min(1.0);

                    let idx = ((py as u32 * width + px as u32) * 4) as usize;
                    if idx + 3 < buffer.len() {
                        let bg_r = buffer[idx + 0] as f32;
                        let bg_g = buffer[idx + 1] as f32;
                        let bg_b = buffer[idx + 2] as f32;

                        buffer[idx + 0] = ((base_color.0 as f32 * alpha) + (bg_r * (1.0 - alpha))) as u8;
                        buffer[idx + 1] = ((base_color.1 as f32 * alpha) + (bg_g * (1.0 - alpha))) as u8;
                        buffer[idx + 2] = ((base_color.2 as f32 * alpha) + (bg_b * (1.0 - alpha))) as u8;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
        }
    }
}
