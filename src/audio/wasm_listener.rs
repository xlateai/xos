use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{AudioContext, AudioProcessingEvent, MediaStream, window};
use std::sync::{Arc, Mutex};
use once_cell::unsync::OnceCell;
use std::collections::VecDeque;

#[derive(Clone)]
pub struct AudioListener {
    buffer: Arc<AudioBuffer>,
}

impl AudioListener {
    pub fn new(_device: &super::wasm_device::AudioDevice, duration_secs: f32) -> Result<Self, String> {
        BUFFER.with(|cell| {
            *cell.borrow_mut() = Some(AudioBuffer::new(duration_secs));
        });

        Ok(Self {
            buffer: BUFFER.with(|cell| {
                cell.borrow().as_ref().unwrap().clone()
            }),
        })
    }

    pub fn record(&self) -> Result<(), String> {
        Ok(())
    }

    pub fn get_samples_by_channel(&self) -> Vec<Vec<f32>> {
        self.buffer.get_samples_by_channel()
    }

    pub fn duration(&self) -> f32 {
        self.buffer.duration()
    }

    pub fn sample_rate(&self) -> u32 {
        self.buffer.sample_rate
    }
}

// Internal audio buffer
#[derive(Clone)]
struct AudioBuffer {
    channels: usize,
    sample_rate: u32,
    capacity: usize,
    channel_buffers: Arc<Mutex<Vec<VecDeque<f32>>>>,
}

impl AudioBuffer {
    fn new(duration_secs: f32) -> Arc<Self> {
        // Default for Web Audio API
        let sample_rate = 44100;
        let channels = 1;
        let capacity = (duration_secs * sample_rate as f32) as usize;

        let mut channel_buffers = Vec::new();
        for _ in 0..channels {
            channel_buffers.push(VecDeque::with_capacity(capacity));
        }

        Arc::new(Self {
            channels,
            sample_rate,
            capacity,
            channel_buffers: Arc::new(Mutex::new(channel_buffers)),
        })
    }

    fn push(&self, samples: &[f32]) {
        let mut buffers = self.channel_buffers.lock().unwrap();
        let buffer = &mut buffers[0];
        for &sample in samples {
            if buffer.len() >= self.capacity {
                buffer.pop_front();
            }
            buffer.push_back(sample);
        }
    }

    fn get_samples_by_channel(&self) -> Vec<Vec<f32>> {
        let buffers = self.channel_buffers.lock().unwrap();
        buffers.iter().map(|b| b.iter().copied().collect()).collect()
    }

    fn duration(&self) -> f32 {
        let buffers = self.channel_buffers.lock().unwrap();
        if buffers.is_empty() {
            0.0
        } else {
            buffers[0].len() as f32 / self.sample_rate as f32
        }
    }
}

thread_local! {
    static BUFFER: std::cell::RefCell<Option<Arc<AudioBuffer>>> = std::cell::RefCell::new(None);
    static AUDIO_CONTEXT: OnceCell<AudioContext> = OnceCell::new();
}

#[wasm_bindgen]
pub async fn init_microphone() -> Result<(), JsValue> {
    let window = window().unwrap();
    let navigator = window.navigator();
    let media_devices = navigator.media_devices()?;

    let constraints = js_sys::Object::new();
    js_sys::Reflect::set(
        &constraints,
        &JsValue::from_str("audio"),
        &JsValue::TRUE,
    )?;

    let stream_promise = media_devices.get_user_media_with_constraints(
        constraints.unchecked_ref()
    )?;

    let stream = wasm_bindgen_futures::JsFuture::from(stream_promise).await?;
    let stream: MediaStream = stream.dyn_into()?;

    let context = AudioContext::new()?;
    let source = context.create_media_stream_source(&stream)?;
    let processor = context.create_script_processor_with_buffer_size(1024)?;

    let closure = Closure::<dyn FnMut(_)>::wrap(Box::new(move |event: AudioProcessingEvent| {
        let input_buf = event.input_buffer().unwrap();
        let input = input_buf.get_channel_data(0).unwrap();

        BUFFER.with(|cell| {
            if let Some(buffer) = &*cell.borrow() {
                buffer.push(&input.to_vec());
            }
        });
    }) as Box<dyn FnMut(_)>);

    processor.set_onaudioprocess(Some(closure.as_ref().unchecked_ref()));
    closure.forget(); // Leak to JS for lifetime safety

    source.connect_with_audio_node(&processor)?;
    processor.connect_with_audio_node(&context.destination())?;

    AUDIO_CONTEXT.with(|ctx| {
        ctx.set(context).ok();
    });

    Ok(())
}
