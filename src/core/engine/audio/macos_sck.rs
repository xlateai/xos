//! macOS system audio via ScreenCaptureKit (macOS 13+). Requires **Screen Recording** permission.

use crate::engine::audio::microphone::AudioBuffer;
use screencapturekit::shareable_content::SCShareableContent;
use screencapturekit::stream::configuration::audio::{AudioChannelCount, AudioSampleRate};
use screencapturekit::stream::configuration::SCStreamConfiguration;
use screencapturekit::stream::content_filter::SCContentFilter;
use screencapturekit::stream::output_trait::SCStreamOutputTrait;
use screencapturekit::stream::output_type::SCStreamOutputType;
use screencapturekit::stream::sc_stream::SCStream;
use screencapturekit::cm::{CMSampleBuffer, SCFrameStatus};

struct SystemAudioHandler {
    buffer: AudioBuffer,
}

impl SCStreamOutputTrait for SystemAudioHandler {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, of_type: SCStreamOutputType) {
        if of_type != SCStreamOutputType::Audio {
            return;
        }
        if matches!(sample.frame_status(), Some(SCFrameStatus::Idle)) {
            return;
        }
        let _ = sample.make_data_ready();
        push_from_sample_buffer(&self.buffer, &sample);
    }
}

fn push_from_sample_buffer(buf: &AudioBuffer, sample: &CMSampleBuffer) {
    let want_ch = buf.channels() as usize;
    if want_ch == 0 {
        return;
    }

    let Some(list) = sample.audio_buffer_list() else {
        return;
    };

    let nb = list.num_buffers();
    if nb == 1 {
        let Some(b) = list.get(0) else {
            return;
        };
        let ch = b.number_channels as usize;
        let data = b.data();
        if ch == want_ch {
            if data.len() % (4 * ch) == 0 {
                let n = data.len() / 4;
                let floats =
                    unsafe { std::slice::from_raw_parts(data.as_ptr().cast::<f32>(), n) };
                buf.push_interleaved_f32(floats, ch as u16);
                return;
            }
            if data.len() % (2 * ch) == 0 {
                let n = data.len() / 2;
                let s16 = unsafe { std::slice::from_raw_parts(data.as_ptr().cast::<i16>(), n) };
                let mut tmp = Vec::with_capacity(n);
                for &s in s16 {
                    tmp.push(s as f32 / i16::MAX as f32);
                }
                buf.push_interleaved_f32(&tmp, ch as u16);
            }
        }
        return;
    }

    if nb == want_ch {
        let mut frame_count: Option<usize> = None;
        for i in 0..nb {
            let Some(b) = list.get(i) else {
                return;
            };
            let d = b.data();
            if d.len() % 4 != 0 {
                return;
            }
            let fc = d.len() / 4;
            frame_count = Some(match frame_count {
                None => fc,
                Some(f) => f.min(fc),
            });
        }
        let Some(frames) = frame_count else {
            return;
        };
        let mut interleaved = Vec::with_capacity(frames * want_ch);
        for fi in 0..frames {
            for ci in 0..want_ch {
                let Some(b) = list.get(ci) else {
                    return;
                };
                let d = b.data();
                let floats = unsafe {
                    std::slice::from_raw_parts(d.as_ptr().cast::<f32>(), d.len() / 4)
                };
                if let Some(&s) = floats.get(fi) {
                    interleaved.push(s);
                }
            }
        }
        buf.push_interleaved_f32(&interleaved, want_ch as u16);
    }
}

/// Build a stream that captures system mix; call `start_capture` when recording.
pub fn build_system_audio_stream(buffer: AudioBuffer) -> Result<SCStream, String> {
    let content =
        SCShareableContent::get().map_err(|e| format!("ScreenCaptureKit content: {e:?}"))?;
    let display = content
        .displays()
        .into_iter()
        .next()
        .ok_or_else(|| "ScreenCaptureKit: no display".to_string())?;
    let w = display.width().max(1);
    let h = display.height().max(1);
    let filter = SCContentFilter::create()
        .with_display(&display)
        .with_excluding_windows(&[])
        .build();
    let config = SCStreamConfiguration::new()
        .with_width(w)
        .with_height(h)
        .with_captures_audio(true)
        .with_sample_rate(AudioSampleRate::Rate48000)
        .with_channel_count(AudioChannelCount::Stereo);

    let mut stream = SCStream::new(&filter, &config);
    stream
        .add_output_handler(
            SystemAudioHandler { buffer },
            SCStreamOutputType::Audio,
        )
        .ok_or_else(|| "ScreenCaptureKit: failed to add audio output handler".to_string())?;
    Ok(stream)
}
