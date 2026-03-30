use crate::engine::{Application, EngineState};
use crate::engine::audio::{devices, default_input, default_output, AudioListener, AudioPlayer, AudioDevice};
use crate::rasterizer::text::text_rasterization::TextRasterizer;
use fontdue::Font;
use std::time::{Instant, Duration};

const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0); // Pitch black background
const CHANNELS: u16 = 1; // Mono input (will be converted to stereo if needed)
const BUFFER_DURATION: f32 = 0.1; // 100ms buffer to prevent overflow between frames (at 60fps = 16.67ms/frame)
const GAIN: f32 = 3.0; // Amplify audio (3x volume boost)

// Toggle button configuration
const BUTTON_SIZE_RATIO: f32 = 0.12; // 12% of smaller screen dimension
const BUTTON_BORDER_WIDTH: f32 = 3.0;

// Menu configuration
const HOLD_DURATION: Duration = Duration::from_millis(250); // 0.25 second hold
const MENU_PADDING: f32 = 20.0;
const MENU_ITEM_HEIGHT: f32 = 50.0; // Slightly larger boxes
const MENU_COLUMN_WIDTH_RATIO: f32 = 0.4; // 40% of screen width per column

pub struct AudioRelay {
    listener: Option<AudioListener>,
    player: Option<AudioPlayer>,
    initialized: bool,
    last_buffer_size: usize,
    enabled: bool, // Audio processing on/off
    // Store device references for recreation
    input_device_index: usize,
    output_device_index: usize,
    use_default_input: bool,
    use_default_output: bool,
    // Menu state
    show_menu: bool,
    mouse_down_time: Option<Instant>,
    // Device lists (cached from setup)
    input_devices: Vec<AudioDevice>,
    output_devices: Vec<AudioDevice>,
    // Font for text rendering
    font: Option<Font>,
}

impl AudioRelay {
    pub fn new() -> Self {
        // Load font
        let font_data = include_bytes!("../../../assets/NotoSans-Medium.ttf");
        let font = Font::from_bytes(font_data as &[u8], fontdue::FontSettings::default()).ok();
        
        Self {
            listener: None,
            player: None,
            initialized: false,
            last_buffer_size: 0,
            enabled: false, // Start disabled (button off)
            input_device_index: 0,
            output_device_index: 0,
            use_default_input: true,  // Default to auto-switching
            use_default_output: true, // Default to auto-switching
            show_menu: false,
            mouse_down_time: None,
            input_devices: Vec::new(),
            output_devices: Vec::new(),
            font,
        }
    }
}

impl Application for AudioRelay {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        crate::print("🎤 AudioRelay - Initializing...");
        
        // Get audio devices
        let all_devices = devices();
        
        // Separate input and output devices
        self.input_devices = all_devices.iter()
            .filter(|d| d.is_input)
            .cloned()
            .collect();
        self.output_devices = all_devices.iter()
            .filter(|d| d.is_output)
            .cloned()
            .collect();
        
        if self.input_devices.is_empty() {
            return Err("No input devices found".to_string());
        }
        if self.output_devices.is_empty() {
            return Err("No output devices found".to_string());
        }
        
        let input_devices: Vec<_> = self.input_devices.iter().collect();
        let output_devices: Vec<_> = self.output_devices.iter().collect();
        
        // Always use default devices initially (can change via menu)
        self.input_device_index = 0;
        self.use_default_input = true;
        self.output_device_index = 0;
        self.use_default_output = true;
        
        crate::print("📍 Input: Default (auto-switching)");
        crate::print("🔊 Output: Default (auto-switching)");
        
        // Create audio devices during setup for instant first toggle
        // Use default devices or specific devices based on selection
        let input_device_owned = if self.use_default_input {
            default_input().ok_or_else(|| "No default input device found".to_string())?
        } else {
            (*input_devices[self.input_device_index]).clone()
        };
        
        let output_device_owned = if self.use_default_output {
            default_output().ok_or_else(|| "No default output device found".to_string())?
        } else {
            (*output_devices[self.output_device_index]).clone()
        };
        
        let input_device = &input_device_owned;
        let output_device = &output_device_owned;
        
        // Create audio listener (starts paused by default)
        let listener = AudioListener::new(input_device, BUFFER_DURATION)
            .map_err(|e| format!("Failed to create listener: {}", e))?;

        // Listener starts paused by default (mic light OFF)
        crate::print("✅ Listener created (paused by default)");
        
        // Get the microphone's actual sample rate and use it for the speaker
        // This prevents pitch/speed issues (e.g., AirPods mic at 16kHz)
        let mic_sample_rate = listener.buffer().sample_rate();
        crate::print(&format!("🎤 Microphone sample rate: {} Hz", mic_sample_rate));
        
        // Create audio player using the microphone's sample rate
        let player = AudioPlayer::new(output_device, mic_sample_rate, CHANNELS)
            .map_err(|e| format!("Failed to create player: {}", e))?;
        
        crate::print(&format!("✅ Player created ({} Hz)", mic_sample_rate));
        
        self.listener = Some(listener);
        self.player = Some(player);
        self.initialized = true;
        
        crate::print("✅ Devices ready!");
        crate::print("   - Quick tap: Toggle audio on/off");
        crate::print("   - Hold 0.25s: Open device menu");
        
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        // Fill background
        let buffer = state.frame_buffer_mut();
        let len = buffer.len();

        for i in (0..len).step_by(4) {
            buffer[i + 0] = BACKGROUND_COLOR.0;
            buffer[i + 1] = BACKGROUND_COLOR.1;
            buffer[i + 2] = BACKGROUND_COLOR.2;
            buffer[i + 3] = 0xff;
        }
        
        // Check if menu should be shown (hold timer)
        if let Some(down_time) = self.mouse_down_time {
            let elapsed = down_time.elapsed();
            if elapsed >= HOLD_DURATION && !self.show_menu {
                self.show_menu = true;
                crate::print("📱 Opening device selection menu...");
            }
        }
        
        // Draw UI
        if self.show_menu {
            self.draw_menu(state);
        } else {
            self.draw_button(state);
        }
        
        // Relay audio if initialized AND enabled
        if self.initialized && self.enabled && !self.show_menu {
            if let (Some(listener), Some(player)) = (&self.listener, &self.player) {
                // Get samples from all channels
                let channels = listener.get_samples_by_channel();
                
                if !channels.is_empty() && !channels[0].is_empty() {
                    // For mono, just use first channel
                    let samples = &channels[0];
                    
                    // Amplify samples for louder output
                    let amplified: Vec<f32> = samples.iter()
                        .map(|&s| (s * GAIN).clamp(-1.0, 1.0)) // Apply gain and clamp to prevent distortion
                        .collect();
                    
                    // Queue amplified samples for playback
                    if let Err(e) = player.play_samples(&amplified) {
                        crate::print(&format!("⚠️  Playback error: {}", e));
                    }
                    
                    // CRITICAL: Clear the listener buffer after reading to avoid re-queueing
                    // the same samples on the next frame!
                    listener.buffer().clear();
                    
                    // Log buffer size occasionally
                    let buffer_size = player.get_buffer_size();
                    if buffer_size != self.last_buffer_size && buffer_size % 1000 == 0 {
                        crate::print(&format!("📊 Buffer: {} samples", buffer_size));
                        self.last_buffer_size = buffer_size;
                    }
                }
            }
        }
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        if !self.initialized {
            return;
        }
        
        // If menu is showing, handle menu interactions or close it
        if self.show_menu {
            // Try to handle menu click - if it returns false, click was outside any button
            if !self.handle_menu_click(state) {
                // Click outside any button - close menu
                self.show_menu = false;
                crate::print("📱 Menu closed");
            }
            return;
        }
        
        // Check if mouse is over the center button
        let shape = state.frame.shape();
        let width = shape[1];
        let height = shape[0];
        let button_size = (width.min(height) as f32 * BUTTON_SIZE_RATIO) as f32;
        let button_x = (width as f32 - button_size) / 2.0;
        let button_y = (height as f32 - button_size) / 2.0;
        
        let mouse_x = state.mouse.x;
        let mouse_y = state.mouse.y;
        
        // Only start hold timer if clicking on the button
        if mouse_x >= button_x && mouse_x <= button_x + button_size
            && mouse_y >= button_y && mouse_y <= button_y + button_size {
            self.mouse_down_time = Some(Instant::now());
        }
    }
    
    fn on_mouse_up(&mut self, state: &mut EngineState) {
        if !self.initialized {
            return;
        }
        
        // Get hold duration
        let hold_duration = self.mouse_down_time
            .map(|t| t.elapsed())
            .unwrap_or(Duration::from_secs(0));
        
        // Clear hold timer
        self.mouse_down_time = None;
        
        // If menu is showing, don't close on release - menu stays open
        if self.show_menu {
            return;
        }
        
        // If it was a quick tap (not a hold), toggle audio
        if hold_duration < HOLD_DURATION {
            // Get button center position
            let shape = state.frame.shape();
            let width = shape[1];
            let height = shape[0];
            let button_size = (width.min(height) as f32 * BUTTON_SIZE_RATIO) as f32;
            let button_x = (width as f32 - button_size) / 2.0;
            let button_y = (height as f32 - button_size) / 2.0;
            
            // Check if click is inside button
            let mouse_x = state.mouse.x;
            let mouse_y = state.mouse.y;
            
            if mouse_x >= button_x && mouse_x <= button_x + button_size
                && mouse_y >= button_y && mouse_y <= button_y + button_size {
                // Toggle enabled state
                self.enabled = !self.enabled;
                
                if self.enabled {
                    // Resume audio processing - INSTANT! (devices already created)
                    if let Some(ref listener) = self.listener {
                        listener.record().ok(); // Resume recording - mic light ON
                    }
                    if let Some(ref player) = self.player {
                        player.start().ok(); // Ensure player is started
                    }
                    crate::print("🟢 Audio relay ENABLED - Mic light ON");
                } else {
                    // Pause audio processing - INSTANT!
                    if let Some(ref listener) = self.listener {
                        listener.pause().ok(); // Pause recording - mic light OFF
                    }
                    if let Some(ref player) = self.player {
                        player.clear(); // Clear speaker buffer immediately
                    }
                    crate::print("⬜ Audio relay DISABLED - Mic light OFF");
                }
            }
        }
    }
    
    fn on_mouse_move(&mut self, _state: &mut EngineState) {
        // No interaction needed
    }
}

impl AudioRelay {
    fn draw_button(&self, state: &mut EngineState) {
        // Get dimensions before borrowing buffer mutably
        let shape = state.frame.shape();
        let height = shape[0];
        let width = shape[1];
        
        // Calculate responsive button size (12% of smaller dimension)
        let button_size = (width.min(height) as f32 * BUTTON_SIZE_RATIO) as usize;
        
        // Center the button at 0.5, 0.5
        let button_x = (width - button_size) / 2;
        let button_y = (height - button_size) / 2;
        
        let buffer = state.frame_buffer_mut();
        
        let x_start = button_x;
        let y_start = button_y;
        let size = button_size;
        let border = BUTTON_BORDER_WIDTH as usize;
        
        // Determine color based on enabled state
        let (r, g, b) = if self.enabled {
            (0, 255, 0) // Green when enabled
        } else {
            (100, 100, 100) // Gray when disabled
        };
        
        for y in y_start..y_start + size {
            if y >= height {
                break;
            }
            
            for x in x_start..x_start + size {
                if x >= width {
                    break;
                }
                
                // Determine if this pixel is part of the border or fill
                let is_border = x < x_start + border 
                    || x >= x_start + size - border
                    || y < y_start + border
                    || y >= y_start + size - border;
                
                // If enabled, fill entire square; if disabled, only draw border
                if self.enabled || is_border {
                    let idx = (y * width + x) * 4;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = r;
                        buffer[idx + 1] = g;
                        buffer[idx + 2] = b;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
        }
    }
    
    fn draw_menu(&self, state: &mut EngineState) {
        let shape = state.frame.shape();
        let height = shape[0];
        let width = shape[1];
        let buffer = state.frame_buffer_mut();
        
        // Calculate menu dimensions
        let column_width = (width as f32 * MENU_COLUMN_WIDTH_RATIO) as usize;
        let gap = 20;
        let left_column_x = (width - column_width * 2 - gap) / 2;
        let right_column_x = left_column_x + column_width + gap;
        let menu_y = MENU_PADDING as usize;
        
        // Draw left column (Input devices)
        self.draw_device_column(
            buffer,
            width,
            height,
            left_column_x,
            menu_y,
            column_width,
            "Input",
            &self.input_devices,
            self.input_device_index,
            self.use_default_input,
        );
        
        // Draw right column (Output devices)
        self.draw_device_column(
            buffer,
            width,
            height,
            right_column_x,
            menu_y,
            column_width,
            "Output",
            &self.output_devices,
            self.output_device_index,
            self.use_default_output,
        );
    }
    
    fn draw_device_column(
        &self,
        buffer: &mut [u8],
        width: usize,
        height: usize,
        x: usize,
        y: usize,
        column_width: usize,
        title: &str,
        devices: &[AudioDevice],
        selected_index: usize,
        use_default: bool,
    ) {
        let item_height = MENU_ITEM_HEIGHT as usize;
        
        // Draw title background (black)
        self.draw_rect(buffer, width, height, x, y, column_width, item_height, (0, 0, 0));
        // Draw title text (white)
        self.draw_text(buffer, width, height, title, x + 10, y + 15, 20.0, (255, 255, 255));
        
        // Draw "Default" option
        let default_y = y + item_height + 5;
        let default_color = if use_default {
            (0, 255, 0) // Neon green for selected
        } else {
            (0, 0, 0) // Black for unselected
        };
        self.draw_rect(buffer, width, height, x, default_y, column_width, item_height, default_color);
        // Draw "Default" text
        let text_color = if use_default { (0, 0, 0) } else { (255, 255, 255) }; // Black on green, white on black
        self.draw_text(buffer, width, height, "Default", x + 10, default_y + 15, 16.0, text_color);
        
        // Draw device options
        for (i, device) in devices.iter().enumerate() {
            let item_y = default_y + item_height + 5 + i * (item_height + 5);
            if item_y + item_height >= height {
                break;
            }
            
            let item_color = if !use_default && i == selected_index {
                (0, 255, 0) // Neon green for selected
            } else {
                (0, 0, 0) // Black for unselected
            };
            
            self.draw_rect(buffer, width, height, x, item_y, column_width, item_height, item_color);
            
            // Draw device name text (truncate if too long)
            let device_name = if device.name.len() > 30 {
                format!("{}...", &device.name[..27])
            } else {
                device.name.clone()
            };
            let text_color = if !use_default && i == selected_index { (0, 0, 0) } else { (255, 255, 255) };
            self.draw_text(buffer, width, height, &device_name, x + 10, item_y + 15, 14.0, text_color);
        }
    }
    
    fn draw_text(
        &self,
        buffer: &mut [u8],
        width: usize,
        height: usize,
        text: &str,
        x: usize,
        y: usize,
        font_size: f32,
        color: (u8, u8, u8),
    ) {
        if let Some(ref font) = self.font {
            let mut rasterizer = TextRasterizer::new(font.clone(), font_size);
            rasterizer.set_text(text.to_string());
            rasterizer.tick(width as f32, height as f32);
            
            for character in &rasterizer.characters {
                let char_x = x as i32 + character.x as i32;
                let char_y = y as i32 + character.y as i32;
                
                for bitmap_y in 0..character.metrics.height {
                    for bitmap_x in 0..character.metrics.width {
                        let alpha = character.bitmap[bitmap_y * character.metrics.width + bitmap_x];
                        
                        if alpha == 0 {
                            continue;
                        }
                        
                        let px = char_x + bitmap_x as i32;
                        let py = char_y + bitmap_y as i32;
                        
                        if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                            let idx = ((py as usize * width + px as usize) * 4) as usize;
                            
                            // Blend with existing pixel using alpha
                            let alpha_f = alpha as f32 / 255.0;
                            let inv_alpha = 1.0 - alpha_f;
                            
                            buffer[idx + 0] = ((color.0 as f32 * alpha_f) + (buffer[idx + 0] as f32 * inv_alpha)) as u8;
                            buffer[idx + 1] = ((color.1 as f32 * alpha_f) + (buffer[idx + 1] as f32 * inv_alpha)) as u8;
                            buffer[idx + 2] = ((color.2 as f32 * alpha_f) + (buffer[idx + 2] as f32 * inv_alpha)) as u8;
                        }
                    }
                }
            }
        }
    }
    
    fn draw_rect(
        &self,
        buffer: &mut [u8],
        width: usize,
        height: usize,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        color: (u8, u8, u8),
    ) {
        for dy in 0..h {
            let py = y + dy;
            if py >= height {
                break;
            }
            
            for dx in 0..w {
                let px = x + dx;
                if px >= width {
                    break;
                }
                
                let idx = (py * width + px) * 4;
                if idx + 3 < buffer.len() {
                    buffer[idx + 0] = color.0;
                    buffer[idx + 1] = color.1;
                    buffer[idx + 2] = color.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }
        
        // Draw border
        let border_color = (200, 200, 200);
        // Top and bottom
        for dx in 0..w {
            let px = x + dx;
            if px < width {
                // Top
                if y < height {
                    let idx = (y * width + px) * 4;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = border_color.0;
                        buffer[idx + 1] = border_color.1;
                        buffer[idx + 2] = border_color.2;
                        buffer[idx + 3] = 0xff;
                    }
                }
                // Bottom
                let bottom_y = y + h - 1;
                if bottom_y < height {
                    let idx = (bottom_y * width + px) * 4;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = border_color.0;
                        buffer[idx + 1] = border_color.1;
                        buffer[idx + 2] = border_color.2;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
        }
        // Left and right
        for dy in 0..h {
            let py = y + dy;
            if py < height {
                // Left
                if x < width {
                    let idx = (py * width + x) * 4;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = border_color.0;
                        buffer[idx + 1] = border_color.1;
                        buffer[idx + 2] = border_color.2;
                        buffer[idx + 3] = 0xff;
                    }
                }
                // Right
                let right_x = x + w - 1;
                if right_x < width {
                    let idx = (py * width + right_x) * 4;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = border_color.0;
                        buffer[idx + 1] = border_color.1;
                        buffer[idx + 2] = border_color.2;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
        }
    }
    
    fn handle_menu_click(&mut self, state: &mut EngineState) -> bool {
        let shape = state.frame.shape();
        let _height = shape[0];
        let width = shape[1];
        let mouse_x = state.mouse.x as usize;
        let mouse_y = state.mouse.y as usize;
        
        // Calculate menu dimensions
        let column_width = (width as f32 * MENU_COLUMN_WIDTH_RATIO) as usize;
        let gap = 20;
        let left_column_x = (width - column_width * 2 - gap) / 2;
        let right_column_x = left_column_x + column_width + gap;
        let menu_y = MENU_PADDING as usize;
        let item_height = MENU_ITEM_HEIGHT as usize;
        
        // Check input column
        if mouse_x >= left_column_x && mouse_x < left_column_x + column_width {
            return self.handle_column_click(
                mouse_y,
                menu_y,
                item_height,
                &self.input_devices.clone(),
                true, // is_input
            );
        }
        
        // Check output column
        if mouse_x >= right_column_x && mouse_x < right_column_x + column_width {
            return self.handle_column_click(
                mouse_y,
                menu_y,
                item_height,
                &self.output_devices.clone(),
                false, // is_output
            );
        }
        
        // Click was outside any column
        false
    }
    
    fn handle_column_click(
        &mut self,
        mouse_y: usize,
        menu_y: usize,
        item_height: usize,
        devices: &[AudioDevice],
        is_input: bool,
    ) -> bool {
        let default_y = menu_y + item_height + 5;
        
        // Check if clicked on "Default"
        if mouse_y >= default_y && mouse_y < default_y + item_height {
            if is_input {
                self.use_default_input = true;
                crate::print("🔄 Switched to default input device");
            } else {
                self.use_default_output = true;
                crate::print("🔄 Switched to default output device");
            }
            self.recreate_audio_devices();
            return true;
        }
        
        // Check device list
        let first_device_y = default_y + item_height + 5;
        if mouse_y >= first_device_y {
            let device_index = (mouse_y - first_device_y) / (item_height + 5);
            if device_index < devices.len() {
                if is_input {
                    self.use_default_input = false;
                    self.input_device_index = device_index;
                    crate::print(&format!("🔄 Switched to input: {}", devices[device_index].name));
                } else {
                    self.use_default_output = false;
                    self.output_device_index = device_index;
                    crate::print(&format!("🔄 Switched to output: {}", devices[device_index].name));
                }
                self.recreate_audio_devices();
                return true;
            }
        }
        
        // Click was outside any button
        false
    }
    
    fn recreate_audio_devices(&mut self) {
        // Pause current audio
        let was_enabled = self.enabled;
        self.enabled = false;
        
        if let Some(ref listener) = self.listener {
            listener.pause().ok();
        }
        if let Some(ref player) = self.player {
            player.clear();
        }
        
        // Drop old devices
        self.listener = None;
        self.player = None;
        
        // Create new devices
        let input_device = if self.use_default_input {
            match default_input() {
                Some(d) => d,
                None => {
                    crate::print("❌ Failed to get default input device");
                    return;
                }
            }
        } else {
            self.input_devices[self.input_device_index].clone()
        };
        
        let output_device = if self.use_default_output {
            match default_output() {
                Some(d) => d,
                None => {
                    crate::print("❌ Failed to get default output device");
                    return;
                }
            }
        } else {
            self.output_devices[self.output_device_index].clone()
        };
        
        // Create listener
        let listener = match AudioListener::new(&input_device, BUFFER_DURATION) {
            Ok(l) => l,
            Err(e) => {
                crate::print(&format!("❌ Failed to create listener: {}", e));
                return;
            }
        };
        
        let mic_sample_rate = listener.buffer().sample_rate();
        
        // Create player
        let player = match AudioPlayer::new(&output_device, mic_sample_rate, CHANNELS) {
            Ok(p) => p,
            Err(e) => {
                crate::print(&format!("❌ Failed to create player: {}", e));
                return;
            }
        };
        
        self.listener = Some(listener);
        self.player = Some(player);
        
        // Restore enabled state
        self.enabled = was_enabled;
        if self.enabled {
            if let Some(ref listener) = self.listener {
                listener.record().ok();
            }
            if let Some(ref player) = self.player {
                player.start().ok();
            }
        }
        
        crate::print("✅ Audio devices recreated successfully");
    }
}

impl Drop for AudioRelay {
    fn drop(&mut self) {
        crate::print("🔌 AudioRelay - Cleaning up...");
        
        // Explicitly drop the audio devices
        self.player = None;
        self.listener = None;
        
        crate::print("✨ AudioRelay - Cleanup complete");
    }
}

