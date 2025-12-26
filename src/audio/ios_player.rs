use crate::audio::ios_device::AudioDevice;

/// Audio player for speaker output (iOS version)
pub struct AudioPlayer {
    player_id: u32,
    sample_rate: u32,
    channels: u16,
}

impl AudioPlayer {
    /// Create a new audio player for the specified device
    pub fn new(audio_device: &AudioDevice, sample_rate: u32, channels: u16) -> Result<Self, String> {
        if !audio_device.is_output {
            return Err("Device is not an output device".to_string());
        }
        
        let player_id = unsafe {
            xos_audio_player_init(audio_device.device_id, sample_rate as f64, channels as u32)
        };
        
        if player_id == u32::MAX {
            return Err("Failed to initialize audio player".to_string());
        }
        
        Ok(Self {
            player_id,
            sample_rate,
            channels,
        })
    }
    
    /// Queue samples for playback
    pub fn play_samples(&self, samples: &[f32]) -> Result<(), String> {
        let result = unsafe {
            xos_audio_player_queue_samples(
                self.player_id,
                samples.as_ptr(),
                samples.len(),
            )
        };
        
        if result != 0 {
            return Err("Failed to queue audio samples".to_string());
        }
        
        Ok(())
    }
    
    /// Get the current buffer size (number of queued samples)
    pub fn get_buffer_size(&self) -> usize {
        unsafe { xos_audio_player_get_buffer_size(self.player_id) as usize }
    }
    
    /// Get the device name (not available in current implementation)
    pub fn device_name(&self) -> &str {
        "iOS Speaker"
    }
    
    /// Start playback
    pub fn start(&self) -> Result<(), String> {
        let result = unsafe { xos_audio_player_start(self.player_id) };
        if result != 0 {
            return Err("Failed to start audio player".to_string());
        }
        Ok(())
    }
    
    /// Stop playback
    #[allow(dead_code)]
    pub fn stop(&self) -> Result<(), String> {
        let result = unsafe { xos_audio_player_stop(self.player_id) };
        if result != 0 {
            return Err("Failed to stop audio player".to_string());
        }
        Ok(())
    }
    
    /// Clear the playback buffer (not implemented in iOS FFI yet)
    #[allow(dead_code)]
    pub fn clear(&self) {
        // TODO: Implement in iOS FFI
    }
    
    /// Get the sample rate
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    
    /// Get the number of channels
    pub fn channels(&self) -> u16 {
        self.channels
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        crate::print(&format!("[AudioPlayer] Destroying player ID={}", self.player_id));
        unsafe {
            xos_audio_player_stop(self.player_id);
            xos_audio_player_destroy(self.player_id);
        }
        crate::print(&format!("[AudioPlayer] Player ID={} destroyed", self.player_id));
    }
}

// FFI declarations for iOS audio player functions
extern "C" {
    fn xos_audio_player_init(
        device_id: u32,
        sample_rate: f64,
        channels: u32,
    ) -> u32;
    
    fn xos_audio_player_queue_samples(
        player_id: u32,
        samples: *const f32,
        count: usize,
    ) -> std::os::raw::c_int;
    
    fn xos_audio_player_get_buffer_size(player_id: u32) -> usize;
    
    fn xos_audio_player_start(player_id: u32) -> std::os::raw::c_int;
    
    fn xos_audio_player_stop(player_id: u32) -> std::os::raw::c_int;
    
    fn xos_audio_player_destroy(player_id: u32);
}

