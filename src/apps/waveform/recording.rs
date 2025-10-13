use cpal::traits::{DeviceTrait, StreamTrait};
use std::sync::Arc;
use std::time::Instant;
use super::waveform::Waveform;

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

pub fn setup_output_stream(waveform: &mut Waveform) -> Result<(), String> {
    let devices = crate::audio::devices();
    if devices.len() < 3 {
        return Err("Not enough audio devices found (need at least 3)".to_string());
    }

    let output_device = &devices[2];
    if !output_device.is_output {
        return Err("Device at index 2 is not an output device".to_string());
    }

    let device = &output_device.device_cpal;
    let config = device.default_output_config()
        .map_err(|e| format!("Failed to get output config: {}", e))?;

    let playback_buffer = Arc::clone(&waveform.playback_buffer);
    let channels = config.channels() as usize;

    let stream = device.build_output_stream(
        &config.into(),
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            let mut buffer = playback_buffer.lock().unwrap();
            
            for chunk in data.chunks_mut(channels) {
                if let Some(sample) = buffer.pop_front() {
                    for channel_data in chunk.iter_mut() {
                        *channel_data = sample;
                    }
                } else {
                    for channel_data in chunk.iter_mut() {
                        *channel_data = 0.0;
                    }
                }
            }
        },
        |err| eprintln!("Output stream error: {}", err),
        None,
    ).map_err(|e| format!("Failed to build output stream: {}", e))?;

    stream.play().map_err(|e| format!("Failed to start output stream: {}", e))?;
    waveform.output_stream = Some(stream);
    Ok(())
}

pub fn stop_output_stream(waveform: &mut Waveform) {
    if let Some(stream) = waveform.output_stream.take() {
        let _ = stream.pause();
    }
    waveform.playback_buffer.lock().unwrap().clear();
}

pub fn feed_playback_buffer(waveform: &Waveform, samples: &[f32]) {
    if waveform.playback_enabled {
        let mut buffer = waveform.playback_buffer.lock().unwrap();
        
        const MAX_BUFFER_SIZE: usize = 2048;
        
        for &sample in samples {
            if buffer.len() < MAX_BUFFER_SIZE {
                buffer.push_back(sample);
            } else {
                buffer.pop_front();
                buffer.push_back(sample);
            }
        }
    }
}

pub fn start_recording(waveform: &mut Waveform) {
    if !waveform.recording_state.is_recording {
        waveform.recording_state.is_recording = true;
        waveform.recording_state.recording_start_time = Some(Instant::now());
        waveform.recording_state.fresh_recording_buffer.clear();
        
        if let Some(listener) = &waveform.listener {
            listener.buffer().clear();
        }
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

pub fn start_replay(waveform: &mut Waveform) {
    if !waveform.recording_state.recorded_samples.is_empty() && !waveform.recording_state.is_replaying {
        waveform.recording_state.is_replaying = true;
        waveform.recording_state.replay_start_time = Some(Instant::now());
        waveform.recording_state.replay_position = 0;
        
        if waveform.output_stream.is_none() {
            if let Err(e) = setup_output_stream(waveform) {
                eprintln!("Failed to setup audio playback for replay: {}", e);
                waveform.recording_state.is_replaying = false;
            }
        }
    }
}

pub fn update_replay(waveform: &mut Waveform) {
    if !waveform.recording_state.is_replaying {
        return;
    }

    if waveform.recording_state.replay_position < waveform.recording_state.recorded_samples.len() {
        let mut buffer = waveform.playback_buffer.lock().unwrap();
        
        while buffer.len() < 1024 && waveform.recording_state.replay_position < waveform.recording_state.recorded_samples.len() {
            buffer.push_back(waveform.recording_state.recorded_samples[waveform.recording_state.replay_position]);
            waveform.recording_state.replay_position += 1;
        }
    }

    if waveform.recording_state.replay_position >= waveform.recording_state.recorded_samples.len() {
        waveform.recording_state.is_replaying = false;
        waveform.recording_state.replay_start_time = None;
        waveform.recording_state.replay_position = 0;
        println!("Replay finished!");
    }
}
