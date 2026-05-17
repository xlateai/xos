use xos_core::engine::{Application, EngineState};
#[cfg(not(target_arch = "wasm32"))]
use crate::apps::audioeditor::track_visualizer::TrackVisualizer;
#[cfg(not(target_os = "linux"))]
use xos_core::rasterizer::shapes::basic_shapes;
#[cfg(not(target_os = "linux"))]
use xos_core::rasterizer::shapes::niche_shapes;

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios"), not(target_os = "linux")))]
use xos_core::engine::audio::{decode_path_to_mono_f32, default_output, AudioPlayer};
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "linux")))]
use std::path::PathBuf;
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "linux")))]
use std::sync::{Arc, Mutex};
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "linux")))]
use std::collections::VecDeque;
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "linux")))]
use std::time::Instant;

const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0); // Black

#[cfg(target_os = "linux")]
pub struct AudioEditApp {
    track_visualizer: TrackVisualizer,
    button_size: f32,
}

#[cfg(target_os = "linux")]
impl AudioEditApp {
    pub fn new() -> Self {
        Self {
            track_visualizer: TrackVisualizer::new(),
            button_size: 60.0,
        }
    }
}

#[cfg(target_os = "linux")]
impl Application for AudioEditApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Err("AudioEdit is unavailable on Linux in this no-audio build".to_string())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let _ = &self.track_visualizer;
        let _ = self.button_size;
        let shape = state.frame.shape();
        let width = shape[1] as u32;
        let height = shape[0] as u32;
        let buffer = state.frame_buffer_mut();
        for y in 0..height {
            for x in 0..width {
                let idx = ((y * width + x) * 4) as usize;
                if idx + 3 < buffer.len() {
                    buffer[idx + 0] = BACKGROUND_COLOR.0;
                    buffer[idx + 1] = BACKGROUND_COLOR.1;
                    buffer[idx + 2] = BACKGROUND_COLOR.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub struct AudioEditApp {
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    mono_pcm: Option<Arc<Vec<f32>>>,
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    audio_player: Option<Arc<AudioPlayer>>,
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    feed_cursor: usize,
    #[cfg(not(target_arch = "wasm32"))]
    track_visualizer: TrackVisualizer,
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(dead_code)]
    audio_samples: Option<Arc<Mutex<VecDeque<f32>>>>,
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(dead_code)]
    total_samples: usize,
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(dead_code)]
    sample_rate: u32,
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(dead_code)]
    original_sample_count: usize,
    #[cfg(not(target_arch = "wasm32"))]
    audio_duration_seconds: f32, // Total audio duration
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(dead_code)]
    audio_file_path: Option<PathBuf>,
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
    zoom_level: f32, // Zoom level (1.0 = full audio, max = 1 second window)
    #[cfg(not(target_arch = "wasm32"))]
    zoom_center: f32, // Center position of zoom (0.0 to 1.0)
    #[cfg(not(target_arch = "wasm32"))]
    is_dragging_zoom_slider: bool, // Whether user is dragging the zoom slider
    button_size: f32, // Size of play/pause button
}

#[cfg(not(target_os = "linux"))]
impl AudioEditApp {
    pub fn new() -> Self {
        Self {
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
            mono_pcm: None,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
            audio_player: None,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
            feed_cursor: 0,
            #[cfg(not(target_arch = "wasm32"))]
            track_visualizer: TrackVisualizer::new(),
            #[cfg(not(target_arch = "wasm32"))]
            audio_samples: None,
            #[cfg(not(target_arch = "wasm32"))]
            total_samples: 0,
            #[cfg(not(target_arch = "wasm32"))]
            sample_rate: 0, // Will be set when audio is loaded
            #[cfg(not(target_arch = "wasm32"))]
            original_sample_count: 0,
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

    /// Load all audio samples from the file for visualization (desktop: Symphonia decode).
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    fn load_full_audio_samples(&mut self, file_path: &PathBuf) -> Result<(), String> {
        let (sample_rate, decoder_duration, mono_vec) = decode_path_to_mono_f32(file_path)
            .map_err(|e| format!("Failed to decode audio file: {e}"))?;
        self.sample_rate = sample_rate;
        self.audio_duration_seconds = decoder_duration;
        self.mono_pcm = Some(Arc::new(mono_vec));
        let all_samples = self.mono_pcm.as_ref().unwrap().as_ref().clone();

        let sample_count = all_samples.len();
        let expected_sample_count = if decoder_duration > 0.0 && sample_rate > 0 {
            (decoder_duration * sample_rate as f32) as usize
        } else {
            sample_count
        };
        let original_count = if sample_count >= 10_000_000 {
            expected_sample_count
        } else {
            all_samples.len()
        };
        let final_samples = if all_samples.len() > 1_000_000 {
            let downsample_factor = (all_samples.len() / 1_000_000) + 1;
            all_samples
                .into_iter()
                .step_by(downsample_factor)
                .collect()
        } else {
            all_samples
        };

        self.track_visualizer.set_samples(final_samples);
        self.track_visualizer.set_original_sample_count(original_count);
        self.original_sample_count = original_count;
        Ok(())
    }

    /// Seek to a specific position in the audio (0.0 to 1.0)
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
        self.playback_start_position = position;
        self.playback_start_time = Some(Instant::now());
        self.zoom_center = position;
        if let Some(player) = &self.audio_player {
            if !self.is_paused {
                let _ = player.start();
            }
        }
        Ok(())
    }

    #[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
    fn seek_audio(&mut self, _position: f32) -> Result<(), String> {
        Ok(())
    }

    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    fn feed_file_playback(&mut self, delta_seconds: f32) {
        let Some(mono) = &self.mono_pcm else {
            return;
        };
        let Some(player) = &self.audio_player else {
            return;
        };
        if self.is_paused {
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
        // Map zoom level (1.0 to max_zoom) to slider position (0.0 to 1.0)
        // Use logarithmic scale for better control
        let max_zoom = if self.audio_duration_seconds > 0.0 {
            self.audio_duration_seconds
        } else {
            100.0 // Fallback
        };
        let zoom_normalized = if max_zoom > 1.0 {
            ((self.zoom_level.ln() - 1.0f32.ln()) / (max_zoom.ln() - 1.0f32.ln())).max(0.0).min(1.0)
        } else {
            0.0
        };
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
                let max_zoom = if self.audio_duration_seconds > 0.0 {
                    self.audio_duration_seconds
                } else {
                    100.0 // Fallback
                };
                let zoom = if max_zoom > 1.0 {
                    (1.0f32.ln() + normalized * (max_zoom.ln() - 1.0f32.ln())).exp()
                } else {
                    1.0
                };
                self.zoom_level = zoom.max(1.0).min(max_zoom);
                return true;
            }
        }

        false
    }

    /// Calculate the visible range based on zoom level and center
    /// Zoom level now represents: 1.0 = full audio, max = 1 second window
    /// Returns (visible_start, visible_end, visible_width)
    #[cfg(not(target_arch = "wasm32"))]
    fn calculate_visible_range(&self) -> (f32, f32, f32) {
        if self.audio_duration_seconds <= 0.0 {
            return (0.0, 1.0, 1.0);
        }
        
        // Calculate max zoom (1 second window)
        let max_zoom = self.audio_duration_seconds;
        
        // Clamp zoom level
        let zoom = self.zoom_level.max(1.0).min(max_zoom);
        
        // Calculate visible window in seconds
        // At zoom 1.0: visible_window = audio_duration_seconds (full audio)
        // At zoom max_zoom: visible_window = 1.0 second
        let visible_window_seconds = self.audio_duration_seconds / zoom;
        
        // Convert to position range (0.0 to 1.0)
        let visible_range = visible_window_seconds / self.audio_duration_seconds;
        
        let ideal_start = self.zoom_center - visible_range / 2.0;
        let ideal_end = self.zoom_center + visible_range / 2.0;
        
        // Clamp to [0.0, 1.0] and adjust center if needed to maintain range size
        let visible_start = ideal_start.max(0.0);
        let visible_end = ideal_end.min(1.0);
        let visible_width = visible_end - visible_start;
        
        (visible_start, visible_end, visible_width)
    }

    /// Convert screen x coordinate to audio position (0.0 to 1.0) accounting for zoom
    #[cfg(not(target_arch = "wasm32"))]
    fn screen_x_to_audio_position(&self, screen_x: f32, screen_width: f32) -> f32 {
        let (visible_start, _, visible_width) = self.calculate_visible_range();
        
        // Normalize screen_x to [0.0, 1.0] within screen width
        let normalized_screen_x = (screen_x / screen_width).max(0.0).min(1.0);
        
        // Map to position within visible range
        let position_in_visible = if visible_width > 0.0 {
            normalized_screen_x
        } else {
            0.5 // Fallback if width is zero
        };
        
        // Convert to actual audio position
        let audio_position = visible_start + position_in_visible * visible_width;
        
        // Clamp to valid range
        audio_position.max(0.0).min(1.0)
    }

    /// Convert audio position (0.0 to 1.0) to screen x coordinate accounting for zoom
    #[cfg(not(target_arch = "wasm32"))]
    fn audio_position_to_screen_x(&self, audio_position: f32, screen_width: f32) -> f32 {
        let (visible_start, visible_end, visible_width) = self.calculate_visible_range();
        
        // Clamp audio position to visible range
        let clamped_position = audio_position.max(visible_start).min(visible_end);
        
        // Map to position within visible range [0.0, 1.0]
        let position_in_visible = if visible_width > 0.0 {
            (clamped_position - visible_start) / visible_width
        } else {
            0.5 // Fallback if width is zero
        };
        
        // Convert to screen x coordinate
        position_in_visible * screen_width
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

        // Map playback position to screen x coordinate
        let position_x = self.audio_position_to_screen_x(self.playback_position, width);
        let line_tolerance = 10.0; // Pixels of tolerance for clicking the line

        // Check if mouse is near the position line
        if (mouse_x - position_x).abs() < line_tolerance {
            if state.mouse.is_left_clicking {
                // Start or continue dragging
                self.is_dragging_position = true;
                // Map mouse_x from screen coordinates to actual audio position accounting for zoom
                let new_position = self.screen_x_to_audio_position(mouse_x, width);
                self.playback_position = new_position;
                return true;
            }
        }

        false
    }
}

#[cfg(not(target_os = "linux"))]
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
                xos_core::print(&format!("Selected file: {:?}", path));
                
                // Store the file path for seeking
                self.audio_file_path = Some(path.clone());
                
                // Load all audio samples for visualization (fills mono_pcm)
                self.load_full_audio_samples(&path)?;

                let sample_rate = self.sample_rate;
                let buffer_capacity = sample_rate.max(8_000) as usize;
                let sample_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(buffer_capacity)));
                self.audio_samples = Some(sample_buffer);

                let out = default_output().ok_or_else(|| "No audio output device found".to_string())?;
                let player = AudioPlayer::new(&out, sample_rate, 2)
                    .map_err(|e| format!("Failed to open audio output: {e}"))?;
                self.audio_player = Some(Arc::new(player));
                self.feed_cursor = 0;
                self.last_seek_position = 0.0;
                self.playback_position = 0.0;
                self.playback_start_position = 0.0;
                self.playback_start_time = Some(Instant::now());
                self.zoom_center = 0.0; // Start zoomed at the beginning

                xos_core::print(&format!("Playing audio file: {:?}", path));
            } else {
                // No audio file selected - close the app
                return Err("No audio file selected. Application will close.".to_string());
            }
        }

        #[cfg(target_os = "ios")]
        {
            xos_core::print("File picker not yet implemented for iOS");
        }

        #[cfg(target_arch = "wasm32")]
        {
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
                    let max_zoom = if self.audio_duration_seconds > 0.0 {
                        self.audio_duration_seconds
                    } else {
                        100.0 // Fallback
                    };
                    let zoom = if max_zoom > 1.0 {
                        (1.0f32.ln() + normalized * (max_zoom.ln() - 1.0f32.ln())).exp()
                    } else {
                        1.0
                    };
                    self.zoom_level = zoom.max(1.0).min(max_zoom);
                } else {
                    // Mouse released
                    self.is_dragging_zoom_slider = false;
                }
            }

            // Handle dragging position line
            if self.is_dragging_position {
                if state.mouse.is_left_clicking {
                    // Continue dragging - map mouse_x to actual audio position accounting for zoom
                    let shape = state.frame.shape();
                    let width = shape[1] as f32;
                    let mouse_x = state.mouse.x;
                    
                    // Use centralized coordinate transformation
                    let new_position = self.screen_x_to_audio_position(mouse_x, width);
                    self.playback_position = new_position;
                } else {
                    // Mouse released, seek to position
                    self.is_dragging_position = false;
                    let position_changed = (self.last_seek_position - self.playback_position).abs() > 0.001;
                    if position_changed {
                        if let Err(e) = self.seek_audio(self.playback_position) {
                            xos_core::print(&format!("Failed to seek audio: {}", e));
                        } else {
                            self.last_seek_position = self.playback_position;
                        }
                    }
                }
            }

            // Update playback position based on elapsed time
            // Account for audio buffering delay (typically 50-100ms)
            const AUDIO_BUFFERING_DELAY_SECONDS: f32 = 0.05; // 50ms delay
            
            if !self.is_dragging_position && !self.is_paused {
                if let Some(start_time) = self.playback_start_time {
                    if self.audio_duration_seconds > 0.0 {
                        // Calculate elapsed time since playback started
                        let elapsed_seconds = start_time.elapsed().as_secs_f32();
                        
                        // Subtract buffering delay to account for audio system latency
                        let adjusted_elapsed = (elapsed_seconds - AUDIO_BUFFERING_DELAY_SECONDS).max(0.0);
                        
                        // Calculate current position: start position + elapsed time as percentage
                        let elapsed_position = adjusted_elapsed / self.audio_duration_seconds;
                        let current_position = (self.playback_start_position + elapsed_position).min(1.0);
                        
                        self.playback_position = current_position;
                    }
                }
            }

            // Control audio playback based on pause state
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
            {
                self.feed_file_playback(state.delta_time_seconds);
                if let Some(player) = &self.audio_player {
                    if self.is_paused {
                        let _ = player.stop();
                    } else {
                        let _ = player.start();
                        if self.playback_start_time.is_none() {
                            self.playback_start_position = self.playback_position;
                            self.playback_start_time = Some(Instant::now());
                        }
                    }
                }
            }

            // Update zoom center to follow playback position when playing
            // Only update if not manually interacting (not dragging position or slider)
            if !self.is_paused && !self.is_dragging_zoom_slider && !self.is_dragging_position {
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

    fn on_mouse_down(&mut self, _state: &mut EngineState) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let state = _state;
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
                // Use centralized coordinate transformation
                let new_position = self.screen_x_to_audio_position(mouse_x, width);
                self.playback_position = new_position;
                self.is_dragging_position = true;
            }
        }
    }
    
    fn on_key_char(&mut self, _state: &mut EngineState, _ch: char) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let ch = _ch;
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
                        xos_core::print(&format!("Failed to seek audio: {}", e));
                    } else {
                        self.last_seek_position = self.playback_position;
                    }
                }
            }
        }
    }
    
    fn on_mouse_move(&mut self, _state: &mut EngineState) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let state = _state;
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
                let max_zoom = if self.audio_duration_seconds > 0.0 {
                    self.audio_duration_seconds
                } else {
                    100.0 // Fallback
                };
                let zoom = if max_zoom > 1.0 {
                    (1.0f32.ln() + normalized * (max_zoom.ln() - 1.0f32.ln())).exp()
                } else {
                    1.0
                };
                self.zoom_level = zoom.max(1.0).min(max_zoom);
            }
            
            // Update position if dragging - map mouse_x to actual audio position accounting for zoom
            if self.is_dragging_position && state.mouse.is_left_clicking {
                let shape = state.frame.shape();
                let width = shape[1] as f32;
                let mouse_x = state.mouse.x;
                
                // Use centralized coordinate transformation
                let new_position = self.screen_x_to_audio_position(mouse_x, width);
                self.playback_position = new_position;
            }
        }
    }
}

