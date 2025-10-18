use std::time::Instant;
use super::waveform::Waveform;
use super::playback::Playback;

pub struct RecordingState {
    pub is_recording: bool,
    pub button_pressed: bool,
    pub recording_start_time: Option<Instant>,
    pub recorded_samples: Vec<f32>,
    pub fresh_recording_buffer: Vec<f32>,
    pub last_processed_length: usize,
    pub is_replaying: bool,
    pub replay_start_time: Option<Instant>,
    pub replay_position: usize,
}

impl RecordingState {
    pub fn new() -> Self {
        Self {
            is_recording: false,
            button_pressed: false,
            recording_start_time: None,
            recorded_samples: Vec::new(),
            fresh_recording_buffer: Vec::new(),
            last_processed_length: 0,
            is_replaying: false,
            replay_start_time: None,
            replay_position: 0,
        }
    }
}

pub fn start_recording(waveform: &mut Waveform) {
    if !waveform.recording_state.is_recording {
        waveform.recording_state.is_recording = true;
        waveform.recording_state.recording_start_time = Some(Instant::now());
        waveform.recording_state.fresh_recording_buffer.clear();
        
        waveform.recording_state.last_processed_length = 0;
        
        println!("🎙️ Push-to-talk recording started (cleared buffer)...");
    }
}

pub fn stop_recording(waveform: &mut Waveform) {
    if waveform.recording_state.is_recording {
        waveform.recording_state.is_recording = false;
        waveform.recording_state.recording_start_time = None;
        waveform.recording_state.recorded_samples = waveform.recording_state.fresh_recording_buffer.clone();
        waveform.recording_state.fresh_recording_buffer.clear();
        println!("🎙️ Recording stopped. Recorded {} samples", waveform.recording_state.recorded_samples.len());
    }
}

pub fn update_recording(waveform: &mut Waveform, samples: &[f32]) {
    if !waveform.recording_state.button_pressed {
        return;
    }

    if let Some(start_time) = waveform.recording_state.recording_start_time {
        if start_time.elapsed().as_secs() >= 5 {
            stop_recording(waveform);
            return;
        }
    }

    if samples.len() > waveform.recording_state.last_processed_length {
        let new_samples = &samples[waveform.recording_state.last_processed_length..];
        waveform.recording_state.fresh_recording_buffer.extend_from_slice(new_samples);
        waveform.recording_state.last_processed_length = samples.len();
        println!("📝 Recording {} samples (total: {})", new_samples.len(), waveform.recording_state.fresh_recording_buffer.len());
    }

    const MAX_RECORDING_SAMPLES: usize = 220_500;
    if waveform.recording_state.fresh_recording_buffer.len() > MAX_RECORDING_SAMPLES {
        waveform.recording_state.fresh_recording_buffer.truncate(MAX_RECORDING_SAMPLES);
        stop_recording(waveform);
    }
}

pub fn start_replay(recording_state: &mut RecordingState, playback: &mut Playback) {
    if !recording_state.recorded_samples.is_empty() && !recording_state.is_replaying {
        recording_state.is_replaying = true;
        recording_state.replay_start_time = Some(Instant::now());
        recording_state.replay_position = 0;
        
        if playback.output_stream.is_none() {
            if let Err(e) = playback.start() {
                eprintln!("Failed to setup audio playback for replay: {}", e);
                recording_state.is_replaying = false;
            }
        }
    }
}

pub fn update_replay(recording_state: &mut RecordingState, playback: &mut Playback) {
    if !recording_state.is_replaying {
        return;
    }

    if recording_state.replay_position < recording_state.recorded_samples.len() {
        let mut buffer = playback.playback_buffer.lock().unwrap();
        
        while buffer.len() < 1024 && recording_state.replay_position < recording_state.recorded_samples.len() {
            buffer.push_back(recording_state.recorded_samples[recording_state.replay_position]);
            recording_state.replay_position += 1;
        }
    }

    if recording_state.replay_position >= recording_state.recorded_samples.len() {
        recording_state.is_replaying = false;
        recording_state.replay_start_time = None;
        recording_state.replay_position = 0;
        println!("Replay finished!");
    }
}
