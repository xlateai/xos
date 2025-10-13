use crate::audio;
use crate::engine::{Application, EngineState};
use super::recording::{self, RecordingState};
use super::visualization;
use super::playback::Playback;

pub struct Waveform {
    pub(crate) listener: Option<audio::AudioListener>,
    pub(crate) playback: Option<Playback>,
    pub(crate) recording_state: RecordingState,
}

impl Waveform {
    pub fn new() -> Self {
        Self {
            listener: None,
            playback: None,
            recording_state: RecordingState::new(),
        }
    }
}

impl Application for Waveform {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        let devices = audio::devices();
        if devices.is_empty() {
            return Err("⚠️ No audio input devices found.".to_string());
        }

        println!("🔊 Available devices:");
        for (i, d) in devices.iter().enumerate() {
            println!("  [{}] {}", i, d.name);
        }

        let device_index = 0;
        let device = devices.get(device_index).ok_or("No audio device found")?;

        let buffer_duration = 1.0;
        let listener = audio::AudioListener::new(device, buffer_duration)?;
        listener.record()?;
        self.listener = Some(listener);

        self.playback = Some(Playback::new()?);

        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let Some(listener) = &self.listener else { return };
        let buffer = &mut state.frame.buffer;

        for pixel in buffer.chunks_exact_mut(4) {
            pixel[0] = 16;
            pixel[1] = 16;
            pixel[2] = 24;
            pixel[3] = 255;
        }

        let all_samples = listener.get_samples_by_channel();
        if all_samples.is_empty() {
            return;
        }

        let samples = &all_samples[0];
        
        if let Some(playback) = &mut self.playback {
            if playback.output_stream.is_some() {
                playback.feed(samples);
            }
        }
        
        visualization::draw_waveform(state, samples);

    recording::update_recording(self, samples);
        
        if let Some(playback) = self.playback.as_mut() {
            recording::update_replay(&mut self.recording_state, playback);
        }

        visualization::draw_toggle_button(self, state);
        visualization::draw_record_button(self, state);
        visualization::draw_replay_button(self, state);
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        if let Some(playback) = &mut self.playback {
            if visualization::is_inside_toggle_button(state.mouse.x, state.mouse.y, state) {
                if playback.output_stream.is_some() {
                    playback.stop();
                } else {
                    if let Err(e) = playback.start() {
                        eprintln!("Failed to start playback: {}", e);
                    }
                }
            }
        }
        
        if visualization::is_inside_record_button(state.mouse.x, state.mouse.y, state) {
            self.recording_state.button_pressed = true;
            recording::start_recording(self);
        } else if visualization::is_inside_replay_button(state.mouse.x, state.mouse.y, state) {
            if self.recording_state.is_replaying {
                self.recording_state.is_replaying = false;
                self.recording_state.replay_position = 0;
            } else {
                if let Some(playback) = self.playback.as_mut() {
                    recording::start_replay(&mut self.recording_state, playback);
                }
            }
        }
    }
    
    fn on_mouse_up(&mut self, _state: &mut EngineState) {
        if self.recording_state.button_pressed {
            self.recording_state.button_pressed = false;
            if self.recording_state.is_recording {
                recording::stop_recording(self);
            }
        }
    }
    
    fn on_mouse_move(&mut self, _state: &mut EngineState) {
    }
}
