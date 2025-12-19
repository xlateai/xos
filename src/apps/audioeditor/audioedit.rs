use crate::engine::{Application, EngineState};
use crate::apps::audioeditor::track_visualizer::TrackVisualizer;
use crate::shapes::basic_shapes;
use crate::shapes::niche_shapes;

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
use std::time::Instant;
#[cfg(not(target_arch = "wasm32"))]
use crate::apps::audiovis::audio_capture::SampleCapturingSource;

const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0); // Black

pub struct AudioEditApp {
    #[cfg(not(target_arch = "wasm32"))]
    sink: Option<Arc<Mutex<Sink>>>, // Keep the sink alive so audio continues playing
    #[cfg(not(target_arch = "wasm32"))]
    _stream: Option<OutputStream>, // Keep the stream alive
    track_visualizer: TrackVisualizer,
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
    #[cfg(not(target_arch = "wasm32"))]
    playback_start_time: Option<Instant>, // When current playback segment started
    #[cfg(not(target_arch = "wasm32"))]
    playback_start_position: f32, // Position when current playback segment started (0.0 to 1.0)
    #[cfg(not(target_arch = "wasm32"))]
    zoom_level: f32, // Zoom level (1.0 to 100.0)
    #[cfg(not(target_arch = "wasm32"))]
    zoom_center: f32, // Center position of zoom (0.0 to 1.0)
    #[cfg(not(target_arch = "wasm32"))]
    is_dragging_zoom_slider: bool, // Whether user is dragging the zoom slider
    button_size: f32, // Size of play/pause button
}

impl AudioEditApp {
    pub fn new() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            sink: None,
            #[cfg(not(target_arch = "wasm32"))]
            _stream: None,
            track_visualizer: TrackVisualizer::new(),
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
            #[cfg(not(target_arch = "wasm32"))]
            playback_start_time: None,
            #[cfg(not(target_arch = "wasm32"))]
            playback_start_position: 0.0,
            #[cfg(not(target_arch = "wasm32"))]
            zoom_level: 1.0,
            #[cfg(not(target_arch = "wasm32"))]
            zoom_center: 0.5,
            #[cfg(not(target_arch = "wasm32"))]
            is_dragging_zoom_slider: false,
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
        let final_samples = if all_samples.len() > 1_000_000 {
            let downsample_factor = (all_samples.len() / 1_000_000) + 1;
            all_samples
                .into_iter()
                .step_by(downsample_factor)
                .collect()
        } else {
            all_samples
        };
        
        #[cfg(not(target_arch = "wasm32"))]
        self.track_visualizer.set_samples(final_samples);
        
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
        
        // Reset playback timing for the new position
        self.playback_start_position = position;
        self.playback_start_time = Some(Instant::now());

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

    /// Render the play/pause button at the center of the screen
    fn render_play_pause_button(&self, state: &mut EngineState) {
        let shape = state.frame.shape();
        let width = shape[1] as u32;
        let height = shape[0] as u32;
        let buffer = state.frame_buffer_mut();

        // Position button at center of screen
        let button_center_x = (width / 2) as f32;
        let button_center_y = (height / 2) as f32;
        let button_radius = (self.button_size / 2.0) * 1.1; // Scale up by 10%

        // Draw button circle with anti-aliasing
        let button_color = (255, 255, 255); // White
        basic_shapes::draw_circle(
            buffer,
            width,
            height,
            button_center_x,
            button_center_y,
            button_radius,
            button_color,
            true, // Enable anti-aliasing
        );

        // Draw play or pause icon
        let icon_color = (0, 0, 0); // Black
        #[cfg(not(target_arch = "wasm32"))]
        if self.is_paused {
            // Draw isosceles triangle pointing right (play icon) with anti-aliasing
            let triangle_width = 20.0 * 1.1; // Scale up by 10%
            let triangle_height = 24.0 * 1.1; // Scale up by 10%
            niche_shapes::draw_play_button(
                buffer,
                width,
                height,
                button_center_x,
                button_center_y,
                triangle_width,
                triangle_height,
                icon_color,
                true, // Enable anti-aliasing
            );
        } else {
            // Draw pause icon (two vertical bars)
            let bar_width = 4.0 * 1.1; // Scale up by 10%
            let bar_height = 18.0 * 1.1; // Scale up by 10%
            let bar_spacing = 8.0 * 1.1; // Scale up by 10%
            
            // Left bar
            let left_bar_x = (button_center_x - bar_spacing / 2.0 - bar_width) as i32;
            let bar_start_y = (button_center_y - bar_height / 2.0) as i32;
            let bar_end_y = (button_center_y + bar_height / 2.0) as i32;
            let bar_width_i = bar_width as i32;
            
            for py in bar_start_y..bar_end_y {
                for px in left_bar_x..(left_bar_x + bar_width_i) {
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
            let right_bar_x = (button_center_x + bar_spacing / 2.0) as i32;
            for py in bar_start_y..bar_end_y {
                for px in right_bar_x..(right_bar_x + bar_width_i) {
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
        #[cfg(target_arch = "wasm32")]
        {
            // On WASM, draw isosceles triangle pointing right (play icon) with anti-aliasing
            let triangle_width = 20.0 * 1.1; // Scale up by 10%
            let triangle_height = 24.0 * 1.1; // Scale up by 10%
            niche_shapes::draw_play_button(
                buffer,
                width,
                height,
                button_center_x,
                button_center_y,
                triangle_width,
                triangle_height,
                icon_color,
                true, // Enable anti-aliasing
            );
        }
    }

    /// Render the zoom slider above the waveform area
    #[cfg(not(target_arch = "wasm32"))]
    fn render_zoom_slider(&self, state: &mut EngineState) {
        let shape = state.frame.shape();
        let width = shape[1] as u32;
        let height = shape[0] as u32;
        let buffer = state.frame_buffer_mut();

        // Get waveform area bounds
        let (waveform_y_start, _) = self.track_visualizer.get_waveform_bounds(width as f32, height as f32);
        
        // Slider is above waveform, centered horizontally
        let slider_height = 30.0;
        let slider_y_start = waveform_y_start - slider_height;
        let slider_y_end = waveform_y_start;
        let slider_width = (width as f32 * 0.4).min(400.0); // 40% of screen width, max 400px
        let slider_x_start = (width as f32 - slider_width) / 2.0;
        let slider_x_end = slider_x_start + slider_width;

        // Draw slider background (dark gray)
        let slider_bg_color = (40, 40, 40);
        for y in (slider_y_start as u32)..(slider_y_end as u32) {
            for x in (slider_x_start as u32)..(slider_x_end as u32) {
                let idx = ((y * width + x) * 4) as usize;
                if idx + 3 < buffer.len() {
                    buffer[idx + 0] = slider_bg_color.0;
                    buffer[idx + 1] = slider_bg_color.1;
                    buffer[idx + 2] = slider_bg_color.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }

        // Draw slider track (lighter gray line)
        let track_color = (100, 100, 100);
        let track_y = (slider_y_start + slider_height / 2.0) as u32;
        for x in (slider_x_start as u32)..(slider_x_end as u32) {
            let idx = ((track_y * width + x) * 4) as usize;
            if idx + 3 < buffer.len() {
                buffer[idx + 0] = track_color.0;
                buffer[idx + 1] = track_color.1;
                buffer[idx + 2] = track_color.2;
                buffer[idx + 3] = 0xff;
            }
        }

        // Draw slider knob
        // Map zoom level (1.0 to 100.0) to slider position (0.0 to 1.0)
        // Use logarithmic scale for better control
        let zoom_normalized = ((self.zoom_level.ln() - 1.0f32.ln()) / (100.0f32.ln() - 1.0f32.ln())).max(0.0).min(1.0);
        let knob_x = slider_x_start + zoom_normalized * slider_width;
        let knob_size = 12.0;
        let knob_color = (200, 200, 255);

        // Draw knob circle
        let knob_y = slider_y_start + slider_height / 2.0;
        for dy in -(knob_size as i32)..=(knob_size as i32) {
            for dx in -(knob_size as i32)..=(knob_size as i32) {
                let dist = ((dx * dx + dy * dy) as f32).sqrt();
                if dist <= knob_size {
                    let x = (knob_x as i32 + dx).max(0).min((width - 1) as i32) as u32;
                    let y = (knob_y as i32 + dy).max(0).min((height - 1) as i32) as u32;
                    let idx = ((y * width + x) * 4) as usize;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = knob_color.0;
                        buffer[idx + 1] = knob_color.1;
                        buffer[idx + 2] = knob_color.2;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
        }
    }

    /// Check if mouse is over the zoom slider and handle dragging
    #[cfg(not(target_arch = "wasm32"))]
    fn handle_zoom_slider_interaction(&mut self, state: &mut EngineState) -> bool {
        let shape = state.frame.shape();
        let width = shape[1] as f32;
        let height = shape[0] as u32;
        let mouse_x = state.mouse.x;
        let mouse_y = state.mouse.y;

        // Get waveform area bounds
        let (waveform_y_start, _) = self.track_visualizer.get_waveform_bounds(width, height as f32);
        
        // Slider area
        let slider_height = 30.0;
        let slider_y_start = waveform_y_start - slider_height;
        let slider_y_end = waveform_y_start;
        let slider_width = (width * 0.4).min(400.0);
        let slider_x_start = (width - slider_width) / 2.0;
        let slider_x_end = slider_x_start + slider_width;

        // Check if mouse is in slider area
        if mouse_y >= slider_y_start && mouse_y <= slider_y_end &&
           mouse_x >= slider_x_start && mouse_x <= slider_x_end {
            if state.mouse.is_left_clicking {
                self.is_dragging_zoom_slider = true;
                // Map mouse x to zoom level (logarithmic)
                let normalized = ((mouse_x - slider_x_start) / slider_width).max(0.0).min(1.0);
                let zoom = (1.0f32.ln() + normalized * (100.0f32.ln() - 1.0f32.ln())).exp();
                self.zoom_level = zoom.max(1.0).min(100.0);
                return true;
            }
        }

        false
    }

    /// Check if mouse is over the position line and handle dragging
    #[cfg(not(target_arch = "wasm32"))]
    fn handle_position_line_interaction(&mut self, state: &mut EngineState) -> bool {
        let shape = state.frame.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        let mouse_x = state.mouse.x;
        let mouse_y = state.mouse.y;

        // Get waveform area bounds
        let (waveform_y_start, _) = self.track_visualizer.get_waveform_bounds(width, height);

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
                self.playback_start_position = 0.0;
                self.playback_start_time = Some(Instant::now());
                self.zoom_center = 0.0; // Start zoomed at the beginning

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
            // Handle dragging zoom slider
            if self.is_dragging_zoom_slider {
                if state.mouse.is_left_clicking {
                    // Continue dragging
                    let shape = state.frame.shape();
                    let width = shape[1] as f32;
                    let mouse_x = state.mouse.x;
                    
                    // Get slider area
                    let slider_width = (width * 0.4).min(400.0);
                    let slider_x_start = (width - slider_width) / 2.0;
                    
                    // Map mouse x to zoom level (logarithmic)
                    let normalized = ((mouse_x - slider_x_start) / slider_width).max(0.0).min(1.0);
                    let zoom = (1.0f32.ln() + normalized * (100.0f32.ln() - 1.0f32.ln())).exp();
                    self.zoom_level = zoom.max(1.0).min(100.0);
                } else {
                    // Mouse released
                    self.is_dragging_zoom_slider = false;
                }
            }

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

            // Update playback position based on elapsed time
            if !self.is_dragging_position && !self.is_paused {
                if let Some(start_time) = self.playback_start_time {
                    if self.audio_duration_seconds > 0.0 {
                        // Calculate elapsed time since playback started
                        let elapsed_seconds = start_time.elapsed().as_secs_f32();
                        
                        // Calculate current position: start position + elapsed time as percentage
                        let elapsed_position = elapsed_seconds / self.audio_duration_seconds;
                        let current_position = (self.playback_start_position + elapsed_position).min(1.0);
                        
                        self.playback_position = current_position;
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
                    // When resuming, reset start time and position to current position
                    // This accounts for the pause duration
                    if self.playback_start_time.is_none() {
                        self.playback_start_position = self.playback_position;
                        self.playback_start_time = Some(Instant::now());
                    }
                }
            }

            // Update zoom center to follow playback position when playing
            if !self.is_paused && !self.is_dragging_zoom_slider {
                self.zoom_center = self.playback_position;
            }

            // Render zoom slider
            self.render_zoom_slider(state);

            // Render waveform and position line
            self.track_visualizer.render(state, self.playback_position, self.zoom_level, self.zoom_center);
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

            // Check play/pause button at center
            let button_center_x = width / 2.0;
            let button_center_y = height / 2.0;
            let button_dist = ((mouse_x - button_center_x).powi(2) + 
                              (mouse_y - button_center_y).powi(2)).sqrt();
            
            if button_dist <= self.button_size / 2.0 {
                self.is_paused = !self.is_paused;
                // When toggling pause, reset timing to current position
                if !self.is_paused {
                    // Resuming: set start position to current and reset timer
                    self.playback_start_position = self.playback_position;
                    self.playback_start_time = Some(Instant::now());
                } else {
                    // Pausing: clear timer so position doesn't update
                    self.playback_start_time = None;
                }
                return;
            }

            // Check zoom slider interaction
            if self.handle_zoom_slider_interaction(state) {
                return;
            }

            // Check position line interaction
            if self.handle_position_line_interaction(state) {
                return;
            }

            // Also allow clicking anywhere in the waveform area to seek
            let (waveform_y_start, _) = self.track_visualizer.get_waveform_bounds(width, height);
            if mouse_y >= waveform_y_start {
                let new_position = (mouse_x / width).max(0.0).min(1.0);
                self.playback_position = new_position;
                self.is_dragging_position = true;
            }
        }
    }
    
    fn on_key_char(&mut self, _state: &mut EngineState, ch: char) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            // Spacebar toggles play/pause
            if ch == ' ' {
                self.is_paused = !self.is_paused;
                // When toggling pause, reset timing to current position
                if !self.is_paused {
                    // Resuming: set start position to current and reset timer
                    self.playback_start_position = self.playback_position;
                    self.playback_start_time = Some(Instant::now());
                } else {
                    // Pausing: clear timer so position doesn't update
                    self.playback_start_time = None;
                }
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
            // Update zoom slider if dragging
            if self.is_dragging_zoom_slider && state.mouse.is_left_clicking {
                let shape = state.frame.shape();
                let width = shape[1] as f32;
                let mouse_x = state.mouse.x;
                
                // Get slider area
                let slider_width = (width * 0.4).min(400.0);
                let slider_x_start = (width - slider_width) / 2.0;
                
                // Map mouse x to zoom level (logarithmic)
                let normalized = ((mouse_x - slider_x_start) / slider_width).max(0.0).min(1.0);
                let zoom = (1.0f32.ln() + normalized * (100.0f32.ln() - 1.0f32.ln())).exp();
                self.zoom_level = zoom.max(1.0).min(100.0);
            }
            
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

