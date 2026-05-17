use xos_core::engine::{Application, EngineState};
#[cfg(not(target_os = "linux"))]
use xos_core::ui::Selector;
#[cfg(not(target_os = "linux"))]
use crate::apps::audiovis::waveform::WaveformVisualizer;
#[cfg(not(target_os = "linux"))]
use crate::apps::audiovis::convolutional_waveform::ConvolutionalWaveform;
#[cfg(not(target_os = "linux"))]
use crate::apps::audiovis::media_control_bar::MediaControlBar;

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios"), not(target_os = "linux")))]
use dialoguer::Select;
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios"), not(target_os = "linux")))]
use xos_core::engine::audio::{
    self, decode_path_to_mono_f32, default_output, AudioPlayer,
};
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "linux")))]
use std::path::PathBuf;
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "linux")))]
use std::sync::{Arc, Mutex};
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "linux")))]
use std::collections::VecDeque;

const BACKGROUND_COLOR: (u8, u8, u8) = (32, 32, 32); // Dark gray
#[cfg(not(target_os = "linux"))]
const BUFFER_SIZE: usize = 512; // Number of audio samples to process per frame

#[cfg(target_os = "linux")]
pub struct AudiovisApp;

#[cfg(target_os = "linux")]
impl AudiovisApp {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(target_os = "linux")]
impl Application for AudiovisApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Err("Audiovis is unavailable on Linux in this no-audio build".to_string())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let buffer = state.frame_buffer_mut();
        let len = buffer.len();
        for i in (0..len).step_by(4) {
            buffer[i + 0] = BACKGROUND_COLOR.0;
            buffer[i + 1] = BACKGROUND_COLOR.1;
            buffer[i + 2] = BACKGROUND_COLOR.2;
            buffer[i + 3] = 0xff;
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub struct AudiovisApp {
    /// Decoded mono PCM for file playback (desktop only).
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    mono_pcm: Option<Arc<Vec<f32>>>,
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    audio_player: Option<Arc<AudioPlayer>>,
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    feed_cursor: usize,
    visual_type_selector: Selector,
    waveform: Option<WaveformVisualizer>,
    convolutional_waveform: Option<ConvolutionalWaveform>,
    media_control_bar: MediaControlBar,
    #[cfg(not(target_arch = "wasm32"))]
    audio_samples: Option<Arc<Mutex<VecDeque<f32>>>>, // Live audio samples buffer
    #[cfg(not(target_arch = "wasm32"))]
    total_samples: usize, // Total samples processed (for position tracking)
    #[cfg(not(target_arch = "wasm32"))]
    sample_rate: u32, // Sample rate for position calculation
    #[cfg(not(target_arch = "wasm32"))]
    audio_duration_seconds: f32, // Total audio duration
    /// Stored when a file is chosen (desktop); seek uses in-memory PCM on desktop.
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(dead_code)]
    audio_file_path: Option<PathBuf>,
    #[cfg(not(target_arch = "wasm32"))]
    last_seek_position: f32, // Last position we seeked to (to detect new seeks)
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    live_listener: Option<audio::AudioListener>,
}

#[cfg(not(target_os = "linux"))]
impl AudiovisApp {
    pub fn new() -> Self {
        Self {
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
            mono_pcm: None,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
            audio_player: None,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
            feed_cursor: 0,
            visual_type_selector: Selector::new(vec![
                "waveform".to_string(),
                "convolution".to_string(),
            ]),
            waveform: None,
            convolutional_waveform: None,
            media_control_bar: MediaControlBar::new(),
            #[cfg(not(target_arch = "wasm32"))]
            audio_samples: None,
            #[cfg(not(target_arch = "wasm32"))]
            total_samples: 0,
            #[cfg(not(target_arch = "wasm32"))]
            sample_rate: 44100,
            #[cfg(not(target_arch = "wasm32"))]
            audio_duration_seconds: 0.0,
            #[cfg(not(target_arch = "wasm32"))]
            audio_file_path: None,
            #[cfg(not(target_arch = "wasm32"))]
            last_seek_position: -1.0,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
            live_listener: None,
        }
    }

    /// Seek to a specific position in the audio (0.0 to 1.0) — desktop file mode (decoded PCM in memory).
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    fn seek_audio(&mut self, position: f32) -> Result<(), String> {
        let position = position.clamp(0.0, 1.0);
        let Some(mono) = &self.mono_pcm else {
            return Err("No audio file loaded".to_string());
        };
        if let Some(player) = &self.audio_player {
            player.clear();
        }
        if let Some(sample_buffer) = &self.audio_samples {
            sample_buffer.lock().unwrap().clear();
        }
        let n = mono.len();
        let target = if n > 0 {
            ((position * n as f32) as usize).min(n.saturating_sub(1))
        } else {
            0
        };
        self.feed_cursor = target;
        self.total_samples = target;
        if let Some(player) = &self.audio_player {
            if !self.media_control_bar.is_paused() {
                let _ = player.start();
            }
        }
        Ok(())
    }

    #[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
    fn seek_audio(&mut self, _position: f32) -> Result<(), String> {
        Ok(())
    }

    /// Push decoded PCM to the output device and visualization buffer (desktop file mode).
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    fn feed_file_playback(&mut self, delta_seconds: f32) {
        if self.live_listener.is_some() {
            return;
        }
        let Some(mono) = &self.mono_pcm else {
            return;
        };
        let Some(player) = &self.audio_player else {
            return;
        };
        if self.media_control_bar.is_paused() {
            return;
        }
        let sr = self.sample_rate.max(1) as f32;
        let n = ((delta_seconds * sr) as usize).max(1);
        let len = mono.len();
        if self.feed_cursor >= len {
            return;
        }
        let end = (self.feed_cursor + n).min(len);
        let chunk = &mono[self.feed_cursor..end];
        if let Some(sb) = &self.audio_samples {
            let mut b = sb.lock().unwrap();
            let cap = self.sample_rate.max(8_000) as usize;
            for &s in chunk {
                b.push_back(s);
                while b.len() > cap {
                    b.pop_front();
                }
            }
        }
        let mut interleaved = Vec::with_capacity(chunk.len() * 2);
        for &s in chunk {
            interleaved.push(s);
            interleaved.push(s);
        }
        let _ = player.play_samples(&interleaved);
        self.feed_cursor = end;
        self.total_samples = self.feed_cursor;
    }
}

#[cfg(not(target_os = "linux"))]
impl Application for AudiovisApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
        {
            let source_items = [
                "Play audio file",
                "Live input (microphone, loopback, or system mix)",
            ];
            let source_choice = Select::new()
                .with_prompt("Audiovis audio source")
                .items(&source_items[..])
                .default(0)
                .interact()
                .map_err(|e| format!("Audio source selection failed: {e}"))?;

            match source_choice {
                0 => {
            // Open file picker for audio files
            let file = rfd::FileDialog::new()
                .add_filter("Audio Files", &["mp3", "wav", "flac", "ogg", "m4a", "aac"])
                .add_filter("All Files", &["*"])
                .pick_file();

            if let Some(path) = file {
                xos_core::print(&format!("Selected file: {:?}", path));
                
                // Store the file path for seeking
                self.audio_file_path = Some(path.clone());

                let (sample_rate, duration, mono_vec) = decode_path_to_mono_f32(&path).map_err(|e| {
                    let extension = path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .unwrap_or("unknown");
                    format!(
                        "Failed to decode audio file (extension: {extension}). Error: {e}. \
                        Supported formats include MP3, WAV, FLAC, OGG, AAC/M4A (Symphonia)."
                    )
                })?;

                self.sample_rate = sample_rate;
                self.audio_duration_seconds = duration;
                self.mono_pcm = Some(Arc::new(mono_vec));

                let cap = sample_rate.max(8_000) as usize;
                let sample_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(cap)));
                self.audio_samples = Some(sample_buffer);

                let out = default_output().ok_or_else(|| "No audio output device found".to_string())?;
                let player = AudioPlayer::new(&out, sample_rate, 2)
                    .map_err(|e| format!("Failed to open audio output: {e}"))?;
                self.audio_player = Some(Arc::new(player));
                self.feed_cursor = 0;
                self.last_seek_position = 0.0;

                xos_core::print(&format!("Playing audio file: {:?}", path));
            } else {
                // No audio file selected - close the app
                return Err("No audio file selected. Application will close.".to_string());
            }
                }
                1 => {
                    let input_devices = audio::all_input_devices();
                    if input_devices.is_empty() {
                        return Err(
                            "No audio input devices found. On Windows, “… (system audio)” entries \
                             should list each output for capture; on macOS use BlackHole or similar."
                                .to_string(),
                        );
                    }
                    let names: Vec<String> = input_devices
                        .iter()
                        .map(|d| d.input_menu_label())
                        .collect();
                    let sel = Select::new()
                        .with_prompt("Select input device")
                        .items(&names)
                        .default(0)
                        .interact()
                        .map_err(|e| format!("Input device selection failed: {e}"))?;
                    let device = input_devices
                        .get(sel)
                        .ok_or_else(|| "Invalid device selection".to_string())?;
                    let buffer_duration = 1.0_f32;
                    let listener = audio::AudioListener::new(device, buffer_duration)?;
                    listener.record()?;
                    self.live_listener = Some(listener);
                    self.media_control_bar.set_live_mode(true);
                    xos_core::print(&format!("Live input: {}", device.name));
                }
                _ => return Err("Invalid audio source selection.".to_string()),
            }
        }

        // Open the selector after the capture/playback source is ready
        self.visual_type_selector.open();

        #[cfg(target_os = "ios")]
        {
            // iOS file picker would go here
            // For now, just log that we're in iOS mode
            xos_core::print("File picker not yet implemented for iOS");
        }

        #[cfg(target_arch = "wasm32")]
        {
            // WASM file picker would go here
            // For now, just log that we're in WASM mode
            xos_core::print("File picker not yet implemented for WASM");
        }

        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let buffer = state.frame_buffer_mut();
        let len = buffer.len();

        // Clear background
        for i in (0..len).step_by(4) {
            buffer[i + 0] = BACKGROUND_COLOR.0;
            buffer[i + 1] = BACKGROUND_COLOR.1;
            buffer[i + 2] = BACKGROUND_COLOR.2;
            buffer[i + 3] = 0xff;
        }

        // Update and render the selector
        let shape = state.frame.shape();
        self.visual_type_selector.update(shape[1] as f32, shape[0] as f32);
        
        // Check if selector is closed and we have a selection
        if !self.visual_type_selector.is_open() {
            if let Some(selected) = self.visual_type_selector.selected_option() {
                match selected {
                    "waveform" => {
                        // Initialize waveform if not already done
                        if self.waveform.is_none() {
                            self.waveform = Some(WaveformVisualizer::new());
                        }
                    }
                    "convolution" => {
                        // Initialize convolutional waveform if not already done
                        if self.convolutional_waveform.is_none() {
                            self.convolutional_waveform = Some(ConvolutionalWaveform::new());
                        }
                    }
                    _ => {}
                }
            }
        } else {
            // Selector is open, render it
            self.visual_type_selector.render(state);
        }

        // Drive file playback and CPAL pause state before we sample the ring for the waveform.
        self.media_control_bar.update(state);

        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
        {
            self.feed_file_playback(state.delta_time_seconds);
            if let Some(player) = &self.audio_player {
                if self.media_control_bar.is_paused() {
                    let _ = player.stop();
                } else {
                    let _ = player.start();
                }
            }
            if let Some(listener) = &self.live_listener {
                if self.media_control_bar.is_paused() {
                    let _ = listener.pause();
                } else {
                    let _ = listener.record();
                }
            }
        }

        // Get audio samples for visualization
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
        let audio_chunk = {
            if let Some(listener) = &self.live_listener {
                let channels = listener.get_samples_by_channel();
                let ch0 = channels.get(0).map(|v| v.as_slice()).unwrap_or(&[]);
                let buffer_len = ch0.len();
                if buffer_len > 0 {
                    let start = buffer_len.saturating_sub(BUFFER_SIZE);
                    let samples: Vec<f32> = ch0.iter().skip(start).copied().collect();
                    let mut chunk = vec![0.0; BUFFER_SIZE];
                    let count = samples.len().min(BUFFER_SIZE);
                    chunk[..count].copy_from_slice(&samples[..count]);
                    Some(chunk)
                } else {
                    Some(vec![0.0; BUFFER_SIZE])
                }
            } else if let Some(sample_buffer) = &self.audio_samples {
                let buffer = sample_buffer.lock().unwrap();
                let buffer_len = buffer.len();

                if buffer_len > 0 {
                    let start = buffer_len.saturating_sub(BUFFER_SIZE);
                    let samples: Vec<f32> = buffer.iter().skip(start).copied().collect();
                    let mut chunk = vec![0.0; BUFFER_SIZE];
                    let count = samples.len().min(BUFFER_SIZE);
                    chunk[..count].copy_from_slice(&samples[..count]);
                    Some(chunk)
                } else {
                    Some(vec![0.0; BUFFER_SIZE])
                }
            } else {
                Some(vec![0.0; BUFFER_SIZE])
            }
        };

        #[cfg(target_os = "ios")]
        let audio_chunk = {
            if let Some(sample_buffer) = &self.audio_samples {
                let buffer = sample_buffer.lock().unwrap();
                let buffer_len = buffer.len();

                if buffer_len > 0 {
                    let start = buffer_len.saturating_sub(BUFFER_SIZE);
                    let samples: Vec<f32> = buffer.iter().skip(start).copied().collect();
                    let mut chunk = vec![0.0; BUFFER_SIZE];
                    let count = samples.len().min(BUFFER_SIZE);
                    chunk[..count].copy_from_slice(&samples[..count]);
                    Some(chunk)
                } else {
                    Some(vec![0.0; BUFFER_SIZE])
                }
            } else {
                Some(vec![0.0; BUFFER_SIZE])
            }
        };

        #[cfg(target_arch = "wasm32")]
        let audio_chunk = Some(vec![0.0; BUFFER_SIZE]);

        // Note: Seeking is handled in on_mouse_up to avoid blocking during drag

        // Update position based on actual audio playback
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(sample_buffer) = &self.audio_samples {
                let buffer = sample_buffer.lock().unwrap();
                let buffer_len = buffer.len();
                
                // Estimate position based on samples processed
                // This is approximate since we're tracking samples in the buffer
                if self.audio_duration_seconds > 0.0 && self.sample_rate > 0 {
                    // Estimate: we've processed (total_samples) samples
                    // Position = samples_processed / (duration * sample_rate)
                    // For now, use a simple approximation based on buffer state
                    // In a real implementation, you'd track the actual playback position
                    let estimated_position = if buffer_len > 0 {
                        // Rough estimate: assume we're at some point in playback
                        // This is a simplified approach - in reality you'd track elapsed time
                        (self.total_samples as f32) / (self.audio_duration_seconds * self.sample_rate as f32)
                    } else {
                        // If buffer is empty, we might be at the end
                        self.media_control_bar.position()
                    };
                    
                    // Only auto-update position when playing and position updates are allowed
                    // This prevents position from snapping back after user seeks
                    // Also, don't update if user has manually seeked recently
                    if !self.media_control_bar.is_paused() 
                       && self.media_control_bar.allow_position_update() 
                       && !self.media_control_bar.is_dragging() {
                        self.media_control_bar.set_position(estimated_position.min(1.0));
                    }
                }
            }
        }

        // Seek position seeds file-mode visualization; live input uses a fixed seed.
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
        let seek_position = if self.live_listener.is_some() {
            0.0
        } else {
            self.media_control_bar.position()
        };
        #[cfg(any(target_arch = "wasm32", target_os = "ios"))]
        let seek_position = self.media_control_bar.position();

        // If waveform is selected and initialized, render it
        if let Some(waveform) = &mut self.waveform {
            if let Some(ref samples) = audio_chunk {
                waveform.update_samples(samples);
            }
            // Always use seek position for randomization (this makes seeking work)
            waveform.tick_with_seed(state, seek_position);
        }

        // If convolutional waveform is selected and initialized, render it
        if let Some(conv_waveform) = &mut self.convolutional_waveform {
            if let Some(ref samples) = audio_chunk {
                conv_waveform.update_samples(samples);
            }
            // Always use seek position for randomization (this makes seeking work)
            conv_waveform.tick_with_seed(state, seek_position);
        }
        
        // Update total samples processed
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(sample_buffer) = &self.audio_samples {
                let buffer = sample_buffer.lock().unwrap();
                // Rough tracking: increment based on buffer updates
                // In reality, you'd track this more accurately
                self.total_samples = self.total_samples.max(buffer.len());
            }
        }

        // Render media control bar only if a visualization is selected
        let has_visualization = self.waveform.is_some() || self.convolutional_waveform.is_some();
        if has_visualization {
            self.media_control_bar.render(state);
        }
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        // Only handle control bar if visualization is selected
        let has_visualization = self.waveform.is_some() || self.convolutional_waveform.is_some();
        if has_visualization {
            // Try media control bar first
            if self.media_control_bar.on_mouse_down(state) {
                // Update pause state if play/pause button was clicked
                #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
                {
                    if !self.media_control_bar.is_dragging() {
                        if let Some(player) = &self.audio_player {
                            if self.media_control_bar.is_paused() {
                                let _ = player.stop();
                            } else {
                                let _ = player.start();
                            }
                        }
                        if let Some(listener) = &self.live_listener {
                            if self.media_control_bar.is_paused() {
                                let _ = listener.pause();
                            } else {
                                let _ = listener.record();
                            }
                        }
                    }
                    // If dragging, we'll seek on mouse up instead
                }
                return;
            }
        }
        // Forward mouse down to the selector
        self.visual_type_selector.on_mouse_down(state);
    }
    
    fn on_mouse_up(&mut self, _state: &mut EngineState) {
        // When user releases mouse after dragging or clicking seek bar, seek to final position
        #[cfg(not(target_arch = "wasm32"))]
        {
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
            if self.live_listener.is_some() {
                return;
            }
            let position = self.media_control_bar.position();
            let position_changed = (self.last_seek_position - position).abs() > 0.001;
            
            // Seek if position changed significantly (user clicked or dragged seek bar)
            if position_changed && !self.media_control_bar.allow_position_update() {
                // User just finished seeking - seek to final position
                if let Err(e) = self.seek_audio(position) {
                    xos_core::print(&format!("Failed to seek audio: {}", e));
                } else {
                    self.last_seek_position = position;
                }
            }
        }
    }
    
    fn on_mouse_move(&mut self, state: &mut EngineState) {
        // Update seek position if dragging
        self.media_control_bar.update_seek_from_mouse(state);
    }
}
