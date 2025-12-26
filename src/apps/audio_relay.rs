use crate::engine::{Application, EngineState};
use crate::audio::{devices, AudioListener, AudioPlayer};

const BACKGROUND_COLOR: (u8, u8, u8) = (20, 20, 20); // Dark background
const SAMPLE_RATE: u32 = 44100;
const CHANNELS: u16 = 1;
const BUFFER_DURATION: f32 = 0.05; // 50ms

pub struct AudioRelay {
    listener: Option<AudioListener>,
    player: Option<AudioPlayer>,
    initialized: bool,
    last_buffer_size: usize,
}

impl AudioRelay {
    pub fn new() -> Self {
        Self {
            listener: None,
            player: None,
            initialized: false,
            last_buffer_size: 0,
        }
    }
}

impl Application for AudioRelay {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        crate::print("🎤 AudioRelay - Initializing...");
        
        // Get audio devices
        let all_devices = devices();
        
        // Find input and output devices
        let input_device = all_devices.iter()
            .find(|d| d.is_input)
            .ok_or("No input device found")?;
        
        let output_device = all_devices.iter()
            .find(|d| d.is_output)
            .ok_or("No output device found")?;
        
        crate::print(&format!("📍 Input: {}", input_device.name));
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
        
        crate::print("🎙️  LIVE - Audio relay active!");
        
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
        
        // Relay audio if initialized
        if self.initialized {
            if let (Some(listener), Some(player)) = (&self.listener, &self.player) {
                // Get samples from all channels
                let channels = listener.get_samples_by_channel();
                
                if !channels.is_empty() && !channels[0].is_empty() {
                    // For mono, just use first channel
                    let samples = &channels[0];
                    
                    // Queue samples for playback
                    if let Err(e) = player.play_samples(samples) {
                        crate::print(&format!("⚠️  Playback error: {}", e));
                    }
                    
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

    fn on_mouse_down(&mut self, _state: &mut EngineState) {
        // No interaction
    }
    
    fn on_mouse_up(&mut self, _state: &mut EngineState) {
        // No interaction
    }
    
    fn on_mouse_move(&mut self, _state: &mut EngineState) {
        // No interaction
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

