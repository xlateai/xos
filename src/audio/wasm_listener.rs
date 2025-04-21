use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{AudioContext, AudioProcessingEvent, MediaStream, window};
use std::sync::{Arc, Mutex};
use once_cell::unsync::OnceCell;

#[derive(Clone)]
pub struct AudioListener {
    buffer: Arc<Mutex<Vec<f32>>>,
}

impl AudioListener {
    pub fn new(_device: &super::wasm_device::AudioDevice, _duration_secs: f32) -> Result<Self, String> {
        Ok(Self {
            buffer: BUFFER.with(|b| b.clone()),
        })
    }

    pub fn record(&self) -> Result<(), String> {
        Ok(())
    }

    pub fn get_samples_by_channel(&self) -> Vec<Vec<f32>> {
        vec![self.buffer.lock().unwrap().clone()]
    }
}

thread_local! {
    static BUFFER: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(vec![0.0; 1024]));
    static AUDIO_CONTEXT: OnceCell<AudioContext> = OnceCell::new();
}

#[wasm_bindgen]
pub async fn init_microphone() -> Result<(), JsValue> {
    let window = window().unwrap();
    let navigator = window.navigator();
    let media_devices = navigator.media_devices()?;

    let mut constraints = js_sys::Object::new();
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

        BUFFER.with(|shared| {
            let mut buffer = shared.lock().unwrap();
            buffer.clear();
            buffer.extend(input.iter().cloned());
        });
    }) as Box<dyn FnMut(_)>);

    processor.set_onaudioprocess(Some(closure.as_ref().unchecked_ref()));
    closure.forget(); // Pin JS closure

    source.connect_with_audio_node(&processor)?;
    processor.connect_with_audio_node(&context.destination())?;

    AUDIO_CONTEXT.with(|ctx| {
        ctx.set(context).ok();
    });

    Ok(())
}
