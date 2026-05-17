//! Decode audio files to mono **f32** PCM using Symphonia (replaces rodio-based decoding).

use std::fs::File;
use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Decode an audio file to mono f32 samples. Returns `(sample_rate, duration_seconds, samples)`.
pub fn decode_path_to_mono_f32(path: &Path) -> Result<(u32, f32, Vec<f32>), String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| e.to_string())?;

    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| "No supported audio tracks".to_string())?;

    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(48_000);

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| e.to_string())?;

    let mut mono: Vec<f32> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(symphonia::core::errors::Error::ResetRequired) => {
                return Err("Audio stream requires reset (unsupported)".to_string());
            }
            Err(e) => return Err(e.to_string()),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(e.to_string()),
        };

        let spec = *decoded.spec();
        let n_capacity = decoded.capacity() as u64;
        if n_capacity == 0 {
            continue;
        }
        let mut sample_buf = SampleBuffer::<f32>::new(n_capacity, spec);
        sample_buf.copy_interleaved_ref(decoded);
        let samples = sample_buf.samples();
        let ch = spec.channels.count();
        if ch == 0 {
            continue;
        }
        let frames = samples.len() / ch;
        for frame_idx in 0..frames {
            let mut sum = 0.0f32;
            for c in 0..ch {
                sum += samples[frame_idx * ch + c];
            }
            mono.push(sum / ch as f32);
        }
    }

    let duration = if sample_rate > 0 {
        mono.len() as f32 / sample_rate as f32
    } else {
        0.0
    };

    Ok((sample_rate, duration, mono))
}
