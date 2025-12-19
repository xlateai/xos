use crate::engine::{Application, EngineState};

#[cfg(not(target_arch = "wasm32"))]
use rodio::{Decoder, OutputStream, Sink, Source};
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
use rodio::OutputStreamBuilder;
#[cfg(not(target_arch = "wasm32"))]
use std::fs::File;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Arc, Mutex};
#[cfg(not(target_arch = "wasm32"))]
use std::collections::VecDeque;
#[cfg(not(target_arch = "wasm32"))]
use crate::apps::audiovis::audio_capture::SampleCapturingSource;

const BACKGROUND_COLOR: (u8, u8, u8) = (32, 32, 32); // Dark gray
const WAVEFORM_HEIGHT_PERCENT: f32 = 0.15; // Bottom 15% of screen

pub struct AudioEditApp {
    #[cfg(not(target_arch = "wasm32"))]
    sink: Option<Arc<Mutex<Sink>>>, // Keep the sink alive so audio continues playing
    #[cfg(not(target_arch = "wasm32"))]
    _stream: Option<OutputStream>, // Keep the stream alive
    #[cfg(not(target_arch = "wasm32"))]
    full_audio_samples: Vec<f32>, // All audio samples for waveform visualization
    #[cfg(not(target_arch = "wasm32"))]
    audio_samples: Option<Arc<Mutex<VecDeque<f32>>>>, // Live audio samples buffer
    #[cfg(not(target_arch = "wasm32"))]
    total_samples: usize, // Total samples processed (for position tracking)
    #[cfg(not(target_arch = "wasm32"))]
    sample_rate: u32, // Sample rate for position calculation
    #[cfg(not(target_arch = "wasm32"))]
    audio_duration_seconds: f32, // Total audio duration
    #[cfg(not(target_arch = "wasm32"))]
    audio_file_path: Option<PathBuf>, // Path to the audio file (for seeking)
    #[cfg(not(target_arch = "wasm32"))]
    playback_position: f32, // Current playback position (0.0 to 1.0)
    #[cfg(not(target_arch = "wasm32"))]
    is_paused: bool, // Whether playback is paused
    #[cfg(not(target_arch = "wasm32"))]
    is_dragging_position: bool, // Whether user is dragging the position line
    #[cfg(not(target_arch = "wasm32"))]
    last_seek_position: f32, // Last position we seeked to (to detect new seeks)
    button_size: f32, // Size of play/pause button
}

impl AudioEditApp {
    pub fn new() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            sink: None,
            #[cfg(not(target_arch = "wasm32"))]
            _stream: None,
            #[cfg(not(target_arch = "wasm32"))]
            full_audio_samples: Vec::new(),
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
            playback_position: 0.0,
            #[cfg(not(target_arch = "wasm32"))]
            is_paused: false,
            #[cfg(not(target_arch = "wasm32"))]
            is_dragging_position: false,
            #[cfg(not(target_arch = "wasm32"))]
            last_seek_position: -1.0,
            button_size: 60.0,
        }
    }

    /// Load all audio samples from the file for visualization
    #[cfg(not(target_arch = "wasm32"))]
    fn load_full_audio_samples(&mut self, file_path: &PathBuf) -> Result<(), String> {
        let file = File::open(file_path)
            .map_err(|e| format!("Failed to open audio file: {}", e))?;
        
        let decoder = Decoder::try_from(file)
            .map_err(|e| format!("Failed to decode audio file: {}", e))?;

        let sample_rate = decoder.sample_rate();
        let channels = decoder.channels() as usize;
        
        // Collect all samples and average channels
        let mut all_samples = Vec::new();
        let mut channel_buffer = Vec::with_capacity(channels);
        let mut sample_count = 0;
        
        for sample in decoder {
            // Convert to f32
            let sample_f32 = sample.to_f32();
            channel_buffer.push(sample_f32);
            
            // When we have all channels for this frame, average them
            if channel_buffer.len() == channels {
                let avg: f32 = channel_buffer.iter().sum::<f32>() / channels as f32;
                all_samples.push(avg);
                channel_buffer.clear();
                sample_count += 1;
                
                // Limit to prevent memory issues (e.g., 10 minutes at 44.1kHz = ~26M samples)
                // We'll downsample for very long files
                if sample_count > 10_000_000 {
                    break;
                }
            }
        }
        
        // Downsample if needed (take every Nth sample to fit in reasonable memory)
        if all_samples.len() > 1_000_000 {
            let downsample_factor = (all_samples.len() / 1_000_000) + 1;
            self.full_audio_samples = all_samples
                .into_iter()
                .step_by(downsample_factor)
                .collect();
        } else {
            self.full_audio_samples = all_samples;
        }
        
        self.sample_rate = sample_rate;
        Ok(())
    }

    /// Seek to a specific position in the audio (0.0 to 1.0)
    #[cfg(not(target_arch = "wasm32"))]
    fn seek_audio(&mut self, position: f32) -> Result<(), String> {
        let position = position.max(0.0).min(1.0);
        
        // Get the file path
        let file_path = match &self.audio_file_path {
            Some(path) => path.clone(),
            None => return Err("No audio file loaded".to_string()),
        };

        // Calculate target time in seconds
        let target_time_seconds = position * self.audio_duration_seconds;
        let target_samples = (target_time_seconds * self.sample_rate as f32) as usize;

        // Stop and clear the current sink
        if let Some(sink) = &self.sink {
            let sink = sink.lock().unwrap();
            sink.stop();
            sink.clear();
        }

        // Clear the sample buffer
        if let Some(sample_buffer) = &self.audio_samples {
            let mut buffer = sample_buffer.lock().unwrap();
            buffer.clear();
        }

        // Reload the audio file
        let file = File::open(&file_path)
            .map_err(|e| format!("Failed to open audio file for seeking: {}", e))?;
        
        let mut decoder = Decoder::try_from(file)
            .map_err(|e| format!("Failed to decode audio file for seeking: {}", e))?;

        // Skip samples to reach the target position
        let channels = decoder.channels() as usize;
        let samples_to_skip = target_samples * channels;
        
        // Skip samples
        let mut skipped = 0;
        for _ in 0..samples_to_skip {
            if decoder.next().is_none() {
                break;
            }
            skipped += 1;
        }

        // Update total_samples to reflect the seek position
        self.total_samples = if skipped > 0 { skipped / channels } else { 0 };

        // Create a new capturing source from the remaining decoder
        let sample_buffer = self.audio_samples.as_ref().unwrap().clone();
        let capturing_source = SampleCapturingSource::new(decoder, sample_buffer, 44100);

        // Append to sink and play
        if let Some(sink) = &self.sink {
            let sink = sink.lock().unwrap();
            sink.append(capturing_source);
            if !self.is_paused {
                sink.play();
            }
        }

        Ok(())
    }

    /// Render the full waveform at the bottom of the screen
    #[cfg(not(target_arch = "wasm32"))]
    fn render_waveform(&self, state: &mut EngineState) {
        if self.full_audio_samples.is_empty() {
            return;
        }

        let shape = state.frame.shape();
        let width = shape[1] as u32;
        let height = shape[0] as u32;
        let buffer = state.frame_buffer_mut();

        // Calculate waveform area (bottom 15% of screen)
        let waveform_height = (height as f32 * WAVEFORM_HEIGHT_PERCENT) as u32;
        let waveform_y_start = height - waveform_height;
        let waveform_y_center = waveform_y_start + waveform_height / 2;

        // Draw waveform
        let num_samples = self.full_audio_samples.len();
        let samples_per_pixel = (num_samples as f32 / width as f32).max(1.0);
        let amplitude = (waveform_height as f32 * 0.4) as i32; // Use 40% of waveform height for amplitude

        let waveform_color = (180, 180, 180); // Light gray

        for x in 0..width {
            let sample_start = (x as f32 * samples_per_pixel) as usize;
            let sample_end = ((x + 1) as f32 * samples_per_pixel) as usize;
            
            // Find min/max in this pixel range
            let range_end = sample_end.min(num_samples);
            if sample_start < range_end {
                let min_val = self.full_audio_samples[sample_start..range_end].iter().fold(f32::INFINITY, |a, &b| a.min(b));
                let max_val = self.full_audio_samples[sample_start..range_end].iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
                
                // Draw vertical line from min to max
                let y_min = (waveform_y_center as i32 - (max_val * amplitude as f32) as i32).max(waveform_y_start as i32);
                let y_max = (waveform_y_center as i32 - (min_val * amplitude as f32) as i32).min((height - 1) as i32);
                
                for y in y_min..=y_max {
                    let idx = ((y as u32 * width + x) * 4) as usize;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = waveform_color.0;
                        buffer[idx + 1] = waveform_color.1;
                        buffer[idx + 2] = waveform_color.2;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
        }
    }

    /// Render the vertical playback position line
    #[cfg(not(target_arch = "wasm32"))]
    fn render_position_line(&self, state: &mut EngineState) {
        let shape = state.frame.shape();
        let width = shape[1] as u32;
        let height = shape[0] as u32;
        let buffer = state.frame_buffer_mut();

        // Calculate waveform area
        let waveform_height = (height as f32 * WAVEFORM_HEIGHT_PERCENT) as u32;
        let waveform_y_start = height - waveform_height;

        // Calculate position line X coordinate
        let position_x = (self.playback_position * width as f32) as u32;
        let line_color = (255, 100, 100); // Red line

        // Draw vertical line from top of waveform area to bottom
        for y in waveform_y_start..height {
            let idx = ((y * width + position_x) * 4) as usize;
            if idx + 3 < buffer.len() {
                buffer[idx + 0] = line_color.0;
                buffer[idx + 1] = line_color.1;
                buffer[idx + 2] = line_color.2;
                buffer[idx + 3] = 0xff;
            }
        }

        // Draw a thicker line (3 pixels wide) for better visibility
        if position_x > 0 && position_x < width - 1 {
            for offset in [-1, 0, 1] {
                let x = (position_x as i32 + offset).max(0).min((width - 1) as i32) as u32;
                for y in waveform_y_start..height {
                    let idx = ((y * width + x) * 4) as usize;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = line_color.0;
                        buffer[idx + 1] = line_color.1;
                        buffer[idx + 2] = line_color.2;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
        }
    }

    /// Render the play/pause button
    fn render_play_pause_button(&self, state: &mut EngineState) {
        let shape = state.frame.shape();
        let width = shape[1] as u32;
        let height = shape[0] as u32;
        let buffer = state.frame_buffer_mut();

        // Position button in the waveform area, left side
        let waveform_height = (height as f32 * WAVEFORM_HEIGHT_PERCENT) as u32;
        let waveform_y_start = height - waveform_height;
        let button_center_x = (self.button_size / 2.0 + 10.0) as i32;
        let button_center_y = (waveform_y_start + waveform_height / 2) as i32;
        let button_radius = (self.button_size / 2.0) as i32;

        // Draw button circle
        let button_color = (100, 100, 120); // Dark gray-blue
        for dy in -button_radius..=button_radius {
            for dx in -button_radius..=button_radius {
                let dist_sq = dx * dx + dy * dy;
                if dist_sq <= button_radius * button_radius {
                    let px = button_center_x + dx;
                    let py = button_center_y + dy;
                    if px >= 0 && py >= 0 && (px as u32) < width && (py as u32) < height {
                        let idx = ((py as u32 * width + px as u32) * 4) as usize;
                        if idx + 3 < buffer.len() {
                            buffer[idx + 0] = button_color.0;
                            buffer[idx + 1] = button_color.1;
                            buffer[idx + 2] = button_color.2;
                            buffer[idx + 3] = 0xff;
                        }
                    }
                }
            }
        }

        // Draw play or pause icon
        let icon_color = (255, 255, 255);
        #[cfg(target_arch = "wasm32")]
        {
            // On WASM, just draw a simple play icon
            let size: i32 = 16;
            for dy in -size..=size {
                for dx in -size..=size {
                    let in_triangle = dy >= -size / 2 && dy <= size / 2 &&
                                     dx >= -size / 2 && dx <= (size / 2 - dy.abs());
                    if in_triangle {
                        let px = button_center_x + dx;
                        let py = button_center_y + dy;
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
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.is_paused {
            // Draw play triangle (pointing right)
            let size: i32 = 16;
            for dy in -size..=size {
                for dx in -size..=size {
                    let in_triangle = dy >= -size / 2 && dy <= size / 2 &&
                                     dx >= -size / 2 && dx <= (size / 2 - dy.abs());
                    if in_triangle {
                        let px = button_center_x + dx;
                        let py = button_center_y + dy;
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
        } else {
            // Draw pause icon (two vertical bars)
            let bar_width = 4;
            let bar_height = 18;
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
    }

    /// Check if mouse is over the position line and handle dragging
    #[cfg(not(target_arch = "wasm32"))]
    fn handle_position_line_interaction(&mut self, state: &mut EngineState) -> bool {
        let shape = state.frame.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        let mouse_x = state.mouse.x;
        let mouse_y = state.mouse.y;

        // Calculate waveform area
        let waveform_height = height * WAVEFORM_HEIGHT_PERCENT;
        let waveform_y_start = height - waveform_height;

        // Check if mouse is in waveform area
        if mouse_y < waveform_y_start {
            return false;
        }

        // Calculate position line X coordinate
        let position_x = self.playback_position * width;
        let line_tolerance = 10.0; // Pixels of tolerance for clicking the line

        // Check if mouse is near the position line
        if (mouse_x - position_x).abs() < line_tolerance {
            if state.mouse.is_left_clicking {
                // Start or continue dragging
                self.is_dragging_position = true;
                // Update position based on mouse X
                let new_position = (mouse_x / width).max(0.0).min(1.0);
                self.playback_position = new_position;
                return true;
            }
        }

        false
    }
}

// Helper trait to convert samples to f32
#[cfg(not(target_arch = "wasm32"))]
trait ToF32 {
    fn to_f32(self) -> f32;
}

#[cfg(not(target_arch = "wasm32"))]
impl ToF32 for f32 {
    fn to_f32(self) -> f32 {
        self
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ToF32 for i16 {
    fn to_f32(self) -> f32 {
        self as f32 / i16::MAX as f32
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ToF32 for u16 {
    fn to_f32(self) -> f32 {
        (self as f32 / u16::MAX as f32) * 2.0 - 1.0
    }
}

impl Application for AudioEditApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
        {
            // Open file picker for audio files
            let file = rfd::FileDialog::new()
                .add_filter("Audio Files", &["mp3", "wav", "flac", "ogg", "m4a", "aac"])
                .add_filter("All Files", &["*"])
                .pick_file();

            if let Some(path) = file {
                crate::print(&format!("Selected file: {:?}", path));
                
                // Store the file path for seeking
                self.audio_file_path = Some(path.clone());
                
                // Load all audio samples for visualization
                self.load_full_audio_samples(&path)?;
                
                // Get an output stream handle to the default physical sound device
                let _stream = OutputStreamBuilder::open_default_stream()
                    .map_err(|e| format!("Failed to get audio output stream: {}", e))?;

                // Create a sink (a queue for audio playback) connected to the mixer
                let sink = Sink::connect_new(&_stream.mixer());

                // Load the audio file
                let file = File::open(&path)
                    .map_err(|e| format!("Failed to open audio file: {}", e))?;
                
                // Try to decode the audio file
                let decoder = Decoder::try_from(file)
                    .map_err(|e| {
                        let extension = path.extension()
                            .and_then(|ext| ext.to_str())
                            .unwrap_or("unknown");
                        format!(
                            "Failed to decode audio file (extension: {}). Error: {}. \
                            Supported formats: MP3, WAV, FLAC, OGG.",
                            extension, e
                        )
                    })?;

                // Get sample rate and duration
                let sample_rate = decoder.sample_rate();
                let duration = decoder.total_duration()
                    .map(|d| d.as_secs_f32())
                    .unwrap_or(0.0);
                
                self.sample_rate = sample_rate;
                self.audio_duration_seconds = duration;

                // Create a buffer to capture live audio samples
                let sample_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(44100)));
                self.audio_samples = Some(sample_buffer.clone());

                // Wrap the decoder with our capturing source
                let capturing_source = SampleCapturingSource::new(decoder, sample_buffer, 44100);

                // Play the audio
                sink.append(capturing_source);
                sink.play();

                // Store the sink and stream to keep them alive
                self.sink = Some(Arc::new(Mutex::new(sink)));
                self._stream = Some(_stream);
                self.last_seek_position = 0.0;
                self.playback_position = 0.0;

                crate::print(&format!("Playing audio file: {:?}", path));
            } else {
                // No audio file selected - close the app
                return Err("No audio file selected. Application will close.".to_string());
            }
        }

        #[cfg(target_os = "ios")]
        {
            crate::print("File picker not yet implemented for iOS");
        }

        #[cfg(target_arch = "wasm32")]
        {
            crate::print("File picker not yet implemented for WASM");
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

        #[cfg(not(target_arch = "wasm32"))]
        {
            // Handle dragging position line
            if self.is_dragging_position {
                if state.mouse.is_left_clicking {
                    // Continue dragging
                    let shape = state.frame.shape();
                    let width = shape[1] as f32;
                    let mouse_x = state.mouse.x;
                    let new_position = (mouse_x / width).max(0.0).min(1.0);
                    self.playback_position = new_position;
                } else {
                    // Mouse released, seek to position
                    self.is_dragging_position = false;
                    let position_changed = (self.last_seek_position - self.playback_position).abs() > 0.001;
                    if position_changed {
                        if let Err(e) = self.seek_audio(self.playback_position) {
                            crate::print(&format!("Failed to seek audio: {}", e));
                        } else {
                            self.last_seek_position = self.playback_position;
                        }
                    }
                }
            }

            // Update playback position based on actual audio playback
            if let Some(sample_buffer) = &self.audio_samples {
                let buffer = sample_buffer.lock().unwrap();
                let buffer_len = buffer.len();
                
                if self.audio_duration_seconds > 0.0 && self.sample_rate > 0 && !self.is_dragging_position {
                    // Estimate position based on samples processed
                    let estimated_position = if buffer_len > 0 {
                        (self.total_samples as f32) / (self.audio_duration_seconds * self.sample_rate as f32)
                    } else {
                        self.playback_position
                    };
                    
                    // Only auto-update position when playing and not dragging
                    if !self.is_paused {
                        self.playback_position = estimated_position.min(1.0);
                    }
                }
            }

            // Control audio playback based on pause state
            if let Some(sink) = &self.sink {
                let sink = sink.lock().unwrap();
                if self.is_paused {
                    sink.pause();
                } else {
                    sink.play();
                }
            }

            // Update total samples processed
            if let Some(sample_buffer) = &self.audio_samples {
                let buffer = sample_buffer.lock().unwrap();
                self.total_samples = self.total_samples.max(buffer.len());
            }

            // Render waveform
            self.render_waveform(state);
            
            // Render position line
            self.render_position_line(state);
        }

        // Render play/pause button
        self.render_play_pause_button(state);
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let shape = state.frame.shape();
            let width = shape[1] as f32;
            let height = shape[0] as f32;
            let mouse_x = state.mouse.x;
            let mouse_y = state.mouse.y;

            // Check play/pause button
            let waveform_height = (height * WAVEFORM_HEIGHT_PERCENT) as u32;
            let waveform_y_start = height - waveform_height as f32;
            let button_center_x = self.button_size / 2.0 + 10.0;
            let button_center_y = waveform_y_start + waveform_height as f32 / 2.0;
            let button_dist = ((mouse_x - button_center_x).powi(2) + 
                              (mouse_y - button_center_y).powi(2)).sqrt();
            
            if button_dist <= self.button_size / 2.0 {
                self.is_paused = !self.is_paused;
                return;
            }

            // Check position line interaction
            if self.handle_position_line_interaction(state) {
                return;
            }

            // Also allow clicking anywhere in the waveform area to seek
            if mouse_y >= waveform_y_start {
                let new_position = (mouse_x / width).max(0.0).min(1.0);
                self.playback_position = new_position;
                self.is_dragging_position = true;
            }
        }
    }
    
    fn on_mouse_up(&mut self, _state: &mut EngineState) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            // When user releases mouse after dragging, seek to final position
            if self.is_dragging_position {
                self.is_dragging_position = false;
                let position_changed = (self.last_seek_position - self.playback_position).abs() > 0.001;
                if position_changed {
                    if let Err(e) = self.seek_audio(self.playback_position) {
                        crate::print(&format!("Failed to seek audio: {}", e));
                    } else {
                        self.last_seek_position = self.playback_position;
                    }
                }
            }
        }
    }
    
    fn on_mouse_move(&mut self, state: &mut EngineState) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            // Update position if dragging
            if self.is_dragging_position && state.mouse.is_left_clicking {
                let shape = state.frame.shape();
                let width = shape[1] as f32;
                let mouse_x = state.mouse.x;
                let new_position = (mouse_x / width).max(0.0).min(1.0);
                self.playback_position = new_position;
            }
        }
    }
}

