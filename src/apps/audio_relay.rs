use crate::engine::{Application, EngineState};
use crate::audio::{devices, AudioListener, AudioPlayer};

const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0); // Pitch black background
const SAMPLE_RATE: u32 = 44100;
const CHANNELS: u16 = 1;
const BUFFER_DURATION: f32 = 0.1; // 100ms buffer to prevent overflow between frames (at 60fps = 16.67ms/frame)
const GAIN: f32 = 3.0; // Amplify audio (3x volume boost)

// Toggle button configuration
const BUTTON_SIZE_RATIO: f32 = 0.12; // 12% of smaller screen dimension
const BUTTON_BORDER_WIDTH: f32 = 3.0;

pub struct AudioRelay {
    listener: Option<AudioListener>,
    player: Option<AudioPlayer>,
    initialized: bool,
    last_buffer_size: usize,
    enabled: bool, // Audio processing on/off
    // Store device references for recreation
    input_device_index: usize,
    output_device_index: usize,
}

impl AudioRelay {
    pub fn new() -> Self {
        Self {
            listener: None,
            player: None,
            initialized: false,
            last_buffer_size: 0,
            enabled: false, // Start disabled (button off)
            input_device_index: 0,
            output_device_index: 0,
        }
    }
}

impl Application for AudioRelay {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        crate::print("🎤 AudioRelay - Initializing...");
        
        // Get audio devices
        let all_devices = devices();
        
        // Separate input and output devices
        let input_devices: Vec<_> = all_devices.iter()
            .filter(|d| d.is_input)
            .collect();
        let output_devices: Vec<_> = all_devices.iter()
            .filter(|d| d.is_output)
            .collect();
        
        if input_devices.is_empty() {
            return Err("No input devices found".to_string());
        }
        if output_devices.is_empty() {
            return Err("No output devices found".to_string());
        }
        
        // Select input device
        #[cfg(not(target_os = "ios"))]
        let input_idx = {
            use dialoguer::Select;
            
            let device_names: Vec<String> = input_devices.iter()
                .map(|d| d.name.clone())
                .collect();
            
            Select::new()
                .with_prompt("Select input device (microphone)")
                .items(&device_names)
                .default(0)
                .interact()
                .map_err(|e| format!("Failed to select input device: {}", e))?
        };
        
        #[cfg(target_os = "ios")]
        let input_idx = 0;
        
        self.input_device_index = input_idx;
        crate::print(&format!("📍 Input: {}", input_devices[input_idx].name));
        
        // Select output device
        #[cfg(not(target_os = "ios"))]
        let output_idx = {
            use dialoguer::Select;
            
            let device_names: Vec<String> = output_devices.iter()
                .map(|d| d.name.clone())
                .collect();
            
            Select::new()
                .with_prompt("Select output device (speakers)")
                .items(&device_names)
                .default(0)
                .interact()
                .map_err(|e| format!("Failed to select output device: {}", e))?
        };
        
        #[cfg(target_os = "ios")]
        let output_idx = 0;
        
        self.output_device_index = output_idx;
        crate::print(&format!("🔊 Output: {}", output_devices[output_idx].name));
        
        // Don't create audio devices yet - will be created on first toggle
        // This keeps mic light OFF by default and makes startup instant
        self.initialized = true;
        
        crate::print("✅ Setup complete! Click the centered square to start audio relay.");
        
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
        
        // Draw toggle button
        self.draw_button(state);
        
        // Relay audio if initialized AND enabled
        if self.initialized && self.enabled {
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
                // Create audio devices if they don't exist yet
                if self.listener.is_none() || self.player.is_none() {
                    crate::print("⚙️  Creating audio devices...");
                    
                    // Get devices again
                    let all_devices = devices();
                    let input_devices: Vec<_> = all_devices.iter().filter(|d| d.is_input).collect();
                    let output_devices: Vec<_> = all_devices.iter().filter(|d| d.is_output).collect();
                    
                    if self.input_device_index < input_devices.len() && self.output_device_index < output_devices.len() {
                        let input_device = input_devices[self.input_device_index];
                        let output_device = output_devices[self.output_device_index];
                        
                        // Create listener
                        match AudioListener::new(input_device, BUFFER_DURATION) {
                            Ok(listener) => {
                                self.listener = Some(listener);
                                crate::print("✅ Microphone created");
                            }
                            Err(e) => {
                                crate::print(&format!("❌ Failed to create microphone: {}", e));
                                self.enabled = false;
                                return;
                            }
                        }
                        
                        // Create player
                        match AudioPlayer::new(output_device, SAMPLE_RATE, CHANNELS) {
                            Ok(player) => {
                                if let Err(e) = player.start() {
                                    crate::print(&format!("⚠️  Failed to start player: {}", e));
                                }
                                self.player = Some(player);
                                crate::print("✅ Speaker created");
                            }
                            Err(e) => {
                                crate::print(&format!("❌ Failed to create speaker: {}", e));
                                self.enabled = false;
                                return;
                            }
                        }
                    }
                } else {
                    // Devices already exist, just resume
                    if let Some(ref listener) = self.listener {
                        listener.record().ok();
                    }
                    if let Some(ref player) = self.player {
                        player.start().ok();
                    }
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
    
    fn on_mouse_up(&mut self, _state: &mut EngineState) {
        // No interaction needed
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

