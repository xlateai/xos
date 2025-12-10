use crate::engine::{Application, EngineState};

#[cfg(not(target_arch = "wasm32"))]
use rodio::{Decoder, OutputStream, Sink};
#[cfg(not(target_arch = "wasm32"))]
use std::fs::File;
#[cfg(not(target_arch = "wasm32"))]
use std::io::BufReader;

const BACKGROUND_COLOR: (u8, u8, u8) = (32, 32, 32); // Dark gray

pub struct AudiovisApp {
    #[cfg(not(target_arch = "wasm32"))]
    _sink: Option<Sink>, // Keep the sink alive so audio continues playing
    #[cfg(not(target_arch = "wasm32"))]
    _stream: Option<OutputStream>, // Keep the stream alive
}

impl AudiovisApp {
    pub fn new() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            _sink: None,
            #[cfg(not(target_arch = "wasm32"))]
            _stream: None,
        }
    }
}

impl Application for AudiovisApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            // Open file picker for audio files
            let file = rfd::FileDialog::new()
                .add_filter("Audio Files", &["mp3", "wav", "flac", "ogg", "m4a", "aac"])
                .add_filter("All Files", &["*"])
                .pick_file();

            if let Some(path) = file {
                // Get an output stream handle to the default physical sound device
                let (_stream, stream_handle) = OutputStream::try_default()
                    .map_err(|e| format!("Failed to get audio output stream: {}", e))?;

                // Create a sink (a queue for audio playback)
                let sink = Sink::try_new(&stream_handle)
                    .map_err(|e| format!("Failed to create audio sink: {}", e))?;

                // Load the audio file
                let file = File::open(&path)
                    .map_err(|e| format!("Failed to open audio file: {}", e))?;
                let source = Decoder::new(BufReader::new(file))
                    .map_err(|e| format!("Failed to decode audio file: {}", e))?;

                // Play the audio
                sink.append(source);
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

        for i in (0..len).step_by(4) {
            buffer[i + 0] = BACKGROUND_COLOR.0;
            buffer[i + 1] = BACKGROUND_COLOR.1;
            buffer[i + 2] = BACKGROUND_COLOR.2;
            buffer[i + 3] = 0xff;
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
