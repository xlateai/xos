use crate::audio;
use crate::engine::{Application, EngineState};
use super::recording::{self, RecordingState};
use super::visualization;
use super::playback::Playback;

pub struct Waveform {
    pub(crate) listener: Option<audio::AudioListener>,
    pub(crate) playback: Option<Playback>,
    pub(crate) recording_state: RecordingState,
    pub is_actively_replaying: bool,
    pub is_replaying_recording: bool,
}

impl Waveform {
    pub fn new() -> Self {
        Self {
            listener: None,
            playback: None,
            recording_state: RecordingState::new(),
            is_actively_replaying: false,
            is_replaying_recording: false,
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

        let max_buffer_size = self.listener.as_ref().unwrap().buffer().capacity();
        self.playback = Some(Playback::new(max_buffer_size)?);

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

        // Feed playback buffer with mic samples if actively replaying
        if self.is_actively_replaying {
            if let Some(playback) = &mut self.playback {
                playback.feed(samples);
                // Visualize playback buffer in red
                let playback_samples: Vec<f32> = playback.playback_buffer.lock().unwrap().iter().copied().collect();
                visualization::draw_waveform_red(state, &playback_samples);
            }
        }

        // If replaying recording, feed playback buffer with recorded samples and visualize in red
        if self.is_replaying_recording {
            if let Some(playback) = &mut self.playback {
                playback.feed(&self.recording_state.recorded_samples);
                let playback_samples: Vec<f32> = playback.playback_buffer.lock().unwrap().iter().copied().collect();
                visualization::draw_waveform_red(state, &playback_samples);
            }
        }

        // Draw main waveform (mic input) in green
        visualization::draw_waveform(state, samples);

        recording::update_recording(self, samples);

        if let Some(playback) = self.playback.as_mut() {
            recording::update_replay(&mut self.recording_state, playback);
        }

        visualization::draw_active_replay_button(self, state);
        visualization::draw_record_button(self, state);
        visualization::draw_replay_recording_button(self, state);
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        // Active replay button toggles is_actively_replaying only
        if visualization::is_inside_active_replay_button(state.mouse.x, state.mouse.y, state) {
            self.is_actively_replaying = !self.is_actively_replaying;
            // Optionally start/stop playback stream here if needed
            return;
        }

        // Record button logic
        if visualization::is_inside_record_button(state.mouse.x, state.mouse.y, state) {
            self.recording_state.button_pressed = true;
            recording::start_recording(self);
            return;
        }

        // Replay recording button toggles is_replaying_recording only
        if visualization::is_inside_replay_recording_button(state.mouse.x, state.mouse.y, state) {
            self.is_replaying_recording = !self.is_replaying_recording;
            // Optionally start/stop replay logic here if needed
            return;
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
