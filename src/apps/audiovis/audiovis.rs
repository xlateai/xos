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
use std::sync::{Arc, Mutex};
#[cfg(not(target_arch = "wasm32"))]
use std::collections::VecDeque;
#[cfg(not(target_arch = "wasm32"))]
use crate::apps::audiovis::audio_capture::SampleCapturingSource;

const BACKGROUND_COLOR: (u8, u8, u8) = (32, 32, 32); // Dark gray

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
        }
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
                    // Get the most recent 256 samples
                    let start = buffer_len.saturating_sub(256);
                    let samples: Vec<f32> = buffer.iter().skip(start).copied().collect();
                    let mut chunk = vec![0.0; 256];
                    let count = samples.len().min(256);
                    chunk[..count].copy_from_slice(&samples[..count]);
                    Some(chunk)
                } else {
                    Some(vec![0.0; 256])
                }
            } else {
                Some(vec![0.0; 256])
            }
        };

        #[cfg(target_arch = "wasm32")]
        let audio_chunk = Some(vec![0.0; 256]);

        // Update media control bar
        self.media_control_bar.update(state);

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
                    if !self.media_control_bar.is_paused() && self.media_control_bar.allow_position_update() {
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
                // Update pause state in audio sink
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
                return;
            }
        }
        // Forward mouse down to the selector
        self.visual_type_selector.on_mouse_down(state);
    }
    
    fn on_mouse_up(&mut self, _state: &mut EngineState) {
        // No interaction
    }
    
    fn on_mouse_move(&mut self, state: &mut EngineState) {
        // Update seek position if dragging
        self.media_control_bar.update_seek_from_mouse(state);
    }
}
