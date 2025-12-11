use crate::engine::{Application, EngineState};
use crate::ui::Selector;
use crate::apps::audiovis::waveform::WaveformVisualizer;
use crate::apps::audiovis::convolutional_waveform::ConvolutionalWaveform;
use crate::apps::audiovis::media_control_bar::MediaControlBar;

#[cfg(not(target_arch = "wasm32"))]
use rodio::{Decoder, OutputStream, OutputStreamBuilder, Sink, Source};
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
const BUFFER_SIZE: usize = 512; // Number of audio samples to process per frame

pub struct AudiovisApp {
    #[cfg(not(target_arch = "wasm32"))]
    sink: Option<Arc<Mutex<Sink>>>, // Keep the sink alive so audio continues playing
    #[cfg(not(target_arch = "wasm32"))]
    _stream: Option<OutputStream>, // Keep the stream alive
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
    #[cfg(not(target_arch = "wasm32"))]
    audio_file_path: Option<PathBuf>, // Path to the audio file (for seeking)
    #[cfg(not(target_arch = "wasm32"))]
    last_seek_position: f32, // Last position we seeked to (to detect new seeks)
}

impl AudiovisApp {
    pub fn new() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            sink: None,
            #[cfg(not(target_arch = "wasm32"))]
            _stream: None,
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
        }
    }

    /// Seek to a specific position in the audio (0.0 to 1.0)
    /// Note: This implementation skips samples by consuming them, which can be slow for large seeks.
    /// Rodio's decoder doesn't support native seeking, so this is the best we can do.
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
        // This may be slow for large files, but it's the only way with rodio
        let channels = decoder.channels() as usize;
        let samples_to_skip = target_samples * channels;
        
        // Skip samples - this will take time for large seeks but at least it works
        let mut skipped = 0;
        for _ in 0..samples_to_skip {
            if decoder.next().is_none() {
                // Reached end of file, break early
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
            if !self.media_control_bar.is_paused() {
                sink.play();
            }
        }

        Ok(())
    }
}

impl Application for AudiovisApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        // Open the selector on startup
        self.visual_type_selector.open();

        #[cfg(not(target_arch = "wasm32"))]
        {
            // Open file picker for audio files
            let file = rfd::FileDialog::new()
                .add_filter("Audio Files", &["mp3", "wav", "flac", "ogg", "m4a", "aac"])
                .add_filter("All Files", &["*"])
                .pick_file();

            if let Some(path) = file {
                println!("Selected file: {:?}", path);
                
                // Store the file path for seeking
                self.audio_file_path = Some(path.clone());
                
                // Get an output stream handle to the default physical sound device
                let _stream = OutputStreamBuilder::open_default_stream()
                    .map_err(|e| format!("Failed to get audio output stream: {}", e))?;

                // Create a sink (a queue for audio playback) connected to the mixer
                let sink = Sink::connect_new(&_stream.mixer());

                // Load the audio file
                let file = File::open(&path)
                    .map_err(|e| format!("Failed to open audio file: {}", e))?;
                
                // Try to decode the audio file using the new API (rodio 0.21+)
                // Decoder::try_from auto-detects the format
                let decoder = Decoder::try_from(file)
                    .map_err(|e| {
                        let extension = path.extension()
                            .and_then(|ext| ext.to_str())
                            .unwrap_or("unknown");
                        format!(
                            "Failed to decode audio file (extension: {}). Error: {}. \
                            Supported formats: MP3, WAV, FLAC, OGG. \
                            If your file is M4A/AAC, try converting it to one of the supported formats.",
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
                let sample_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(44100))); // ~1 second at 44.1kHz
                self.audio_samples = Some(sample_buffer.clone());

                // Wrap the decoder with our capturing source
                let capturing_source = SampleCapturingSource::new(decoder, sample_buffer, 44100);

                // Play the audio
                sink.append(capturing_source);
                sink.play();

                // Store the sink and stream to keep them alive (wrap sink in Arc<Mutex> for sharing)
                self.sink = Some(Arc::new(Mutex::new(sink)));
                self._stream = Some(_stream);
                self.last_seek_position = 0.0;

                println!("Playing audio file: {:?}", path);
            } else {
                // No audio file selected - close the app
                return Err("No audio file selected. Application will close.".to_string());
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            // WASM file picker would go here
            // For now, just log that we're in WASM mode
            println!("File picker not yet implemented for WASM");
        }

        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let buffer = &mut state.frame.buffer;
        let len = buffer.len();

        // Clear background
        for i in (0..len).step_by(4) {
            buffer[i + 0] = BACKGROUND_COLOR.0;
            buffer[i + 1] = BACKGROUND_COLOR.1;
            buffer[i + 2] = BACKGROUND_COLOR.2;
            buffer[i + 3] = 0xff;
        }

        // Update and render the selector
        self.visual_type_selector.update(state.frame.width as f32, state.frame.height as f32);
        
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

        // Get audio samples for visualization
        #[cfg(not(target_arch = "wasm32"))]
        let audio_chunk = {
            if let Some(sample_buffer) = &self.audio_samples {
                let buffer = sample_buffer.lock().unwrap();
                let buffer_len = buffer.len();
                
                if buffer_len > 0 {
                    // Get the most recent BUFFER_SIZE samples
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

        // Update media control bar
        self.media_control_bar.update(state);

        // Note: Seeking is handled in on_mouse_up to avoid blocking during drag

        // Control audio playback based on pause state
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(sink) = &self.sink {
                let sink = sink.lock().unwrap();
                if self.media_control_bar.is_paused() {
                    sink.pause();
                } else {
                    sink.play();
                }
            }
        }

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

        // Get seek position for randomization
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
                #[cfg(not(target_arch = "wasm32"))]
                {
                    if !self.media_control_bar.is_dragging() {
                        // User clicked play/pause button - update pause state
                        if let Some(sink) = &self.sink {
                            let sink = sink.lock().unwrap();
                            if self.media_control_bar.is_paused() {
                                sink.pause();
                            } else {
                                sink.play();
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
            let position = self.media_control_bar.position();
            let position_changed = (self.last_seek_position - position).abs() > 0.001;
            
            // Seek if position changed significantly (user clicked or dragged seek bar)
            if position_changed && !self.media_control_bar.allow_position_update() {
                // User just finished seeking - seek to final position
                if let Err(e) = self.seek_audio(position) {
                    eprintln!("Failed to seek audio: {}", e);
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
