use crate::engine::{Application, EngineState};
use crate::ui::Selector;
use crate::apps::audiovis::waveform::WaveformVisualizer;
use crate::apps::audiovis::convolutional_waveform::ConvolutionalWaveform;

#[cfg(not(target_arch = "wasm32"))]
use rodio::{Decoder, OutputStream, OutputStreamBuilder, Sink};
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
    _sink: Option<Sink>, // Keep the sink alive so audio continues playing
    #[cfg(not(target_arch = "wasm32"))]
    _stream: Option<OutputStream>, // Keep the stream alive
    visual_type_selector: Selector,
    waveform: Option<WaveformVisualizer>,
    convolutional_waveform: Option<ConvolutionalWaveform>,
    #[cfg(not(target_arch = "wasm32"))]
    audio_samples: Option<Arc<Mutex<VecDeque<f32>>>>, // Live audio samples buffer
}

impl AudiovisApp {
    pub fn new() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            _sink: None,
            #[cfg(not(target_arch = "wasm32"))]
            _stream: None,
            visual_type_selector: Selector::new(vec![
                "waveform".to_string(),
                "convolution".to_string(),
            ]),
            waveform: None,
            convolutional_waveform: None,
            #[cfg(not(target_arch = "wasm32"))]
            audio_samples: None,
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

                // Create a buffer to capture live audio samples
                let sample_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(44100))); // ~1 second at 44.1kHz
                self.audio_samples = Some(sample_buffer.clone());

                // Wrap the decoder with our capturing source
                let capturing_source = SampleCapturingSource::new(decoder, sample_buffer, 44100);

                // Play the audio
                sink.append(capturing_source);
                sink.play();

                // Store the sink and stream to keep them alive
                self._sink = Some(sink);
                self._stream = Some(_stream);

                println!("Playing audio file: {:?}", path);
            } else {
                println!("No audio file selected");
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
                            // Use a small window to avoid overheating
                            let img_width = 64;
                            let img_height = 64;
                            self.convolutional_waveform = Some(ConvolutionalWaveform::new(img_width, img_height));
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

        // If waveform is selected and initialized, render it
        if let Some(waveform) = &mut self.waveform {
            if let Some(ref samples) = audio_chunk {
                waveform.update_samples(samples);
            }
            waveform.tick(state);
        }

        // If convolutional waveform is selected and initialized, render it
        if let Some(conv_waveform) = &mut self.convolutional_waveform {
            if let Some(ref samples) = audio_chunk {
                conv_waveform.update_samples(samples);
            }
            conv_waveform.tick(state);
        }
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        // Forward mouse down to the selector
        self.visual_type_selector.on_mouse_down(state);
    }
    
    fn on_mouse_up(&mut self, _state: &mut EngineState) {
        // No interaction
    }
    
    fn on_mouse_move(&mut self, _state: &mut EngineState) {
        // No interaction
    }
}
