use crate::engine::{Application, EngineState};
use crate::audio::{devices, AudioListener, AudioPlayer};

const BACKGROUND_COLOR: (u8, u8, u8) = (20, 20, 20); // Dark background
const SAMPLE_RATE: u32 = 44100;
const CHANNELS: u16 = 1;
const BUFFER_DURATION: f32 = 0.1; // 100ms buffer to prevent overflow between frames (at 60fps = 16.67ms/frame)
const GAIN: f32 = 3.0; // Amplify audio (3x volume boost)

// Toggle button configuration
const BUTTON_SIZE: f32 = 60.0;
const BUTTON_PADDING: f32 = 20.0;
const BUTTON_BORDER_WIDTH: f32 = 3.0;

pub struct AudioRelay {
    listener: Option<AudioListener>,
    player: Option<AudioPlayer>,
    initialized: bool,
    last_buffer_size: usize,
    enabled: bool, // Audio processing on/off
    button_x: f32,
    button_y: f32,
}

impl AudioRelay {
    pub fn new() -> Self {
        Self {
            listener: None,
            player: None,
            initialized: false,
            last_buffer_size: 0,
            enabled: false, // Start disabled (button off)
            button_x: BUTTON_PADDING,
            button_y: BUTTON_PADDING,
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
        let input_device = {
            use dialoguer::Select;
            
            let device_names: Vec<String> = input_devices.iter()
                .map(|d| d.name.clone())
                .collect();
            
            let selection = Select::new()
                .with_prompt("Select input device (microphone)")
                .items(&device_names)
                .default(0)
                .interact()
                .map_err(|e| format!("Failed to select input device: {}", e))?;
            
            input_devices[selection]
        };
        
        #[cfg(target_os = "ios")]
        let input_device = input_devices[0];
        
        crate::print(&format!("📍 Input: {}", input_device.name));
        
        // Select output device
        #[cfg(not(target_os = "ios"))]
        let output_device = {
            use dialoguer::Select;
            
            let device_names: Vec<String> = output_devices.iter()
                .map(|d| d.name.clone())
                .collect();
            
            let selection = Select::new()
                .with_prompt("Select output device (speakers)")
                .items(&device_names)
                .default(0)
                .interact()
                .map_err(|e| format!("Failed to select output device: {}", e))?;
            
            output_devices[selection]
        };
        
        #[cfg(target_os = "ios")]
        let output_device = output_devices[0];
        
        crate::print(&format!("🔊 Output: {}", output_device.name));
        
        // Create audio listener
        let listener = AudioListener::new(input_device, BUFFER_DURATION)
            .map_err(|e| format!("Failed to create listener: {}", e))?;
        
        crate::print("✅ Listener created");
        
        // Create audio player
        let player = AudioPlayer::new(output_device, SAMPLE_RATE, CHANNELS)
            .map_err(|e| format!("Failed to create player: {}", e))?;
        
        crate::print("✅ Player created");
        
        self.listener = Some(listener);
        self.player = Some(player);
        self.initialized = true;
        
        crate::print("🎙️  Devices initialized! Click the square to start audio relay.");
        
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
        // Check if click is inside button
        let mouse_x = state.mouse.x;
        let mouse_y = state.mouse.y;
        
        if mouse_x >= self.button_x && mouse_x <= self.button_x + BUTTON_SIZE
            && mouse_y >= self.button_y && mouse_y <= self.button_y + BUTTON_SIZE {
            // Toggle enabled state
            self.enabled = !self.enabled;
            
            if self.enabled {
                crate::print("🟢 Audio relay ENABLED");
            } else {
                crate::print("⬜ Audio relay DISABLED");
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
        
        let buffer = state.frame_buffer_mut();
        
        let x_start = self.button_x as usize;
        let y_start = self.button_y as usize;
        let size = BUTTON_SIZE as usize;
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

