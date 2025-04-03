// src/video/webcam/wasm_webcam.rs

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, HtmlVideoElement, MediaStreamConstraints, window};

use std::cell::RefCell;

thread_local! {
    static VIDEO_ELEMENT: RefCell<Option<HtmlVideoElement>> = RefCell::new(None);
    static CANVAS_ELEMENT: RefCell<Option<HtmlCanvasElement>> = RefCell::new(None);
    static CONTEXT_2D: RefCell<Option<CanvasRenderingContext2d>> = RefCell::new(None);
    static RESOLUTION: RefCell<(u32, u32)> = RefCell::new((640, 480)); // default fallback
}

pub fn init_camera() {
    let window = window().unwrap();
    let document = window.document().unwrap();

    // Create video element
    let video = document
        .create_element("video").unwrap()
        .dyn_into::<HtmlVideoElement>().unwrap();
    video.set_autoplay(true);
    video.set_attribute("playsinline", "true").unwrap();
    video.set_attribute("muted", "true").unwrap(); // silence to avoid permissions issues

    // Request webcam access
    let constraints = MediaStreamConstraints::new();
    constraints.video(&JsValue::TRUE);
    let navigator = window.navigator();
    let media_devices = navigator.media_devices().unwrap();

    let closure = Closure::wrap(Box::new(move |stream: JsValue| {
        let media_stream = stream.dyn_into::<web_sys::MediaStream>().unwrap();
        video.set_src_object(Some(&media_stream));
    }) as Box<dyn FnMut(JsValue)>);

    media_devices
        .get_user_media_with_constraints(&constraints)
        .unwrap()
        .then(&closure);
    closure.forget();

    // Create hidden canvas
    let canvas = document
        .create_element("canvas").unwrap()
        .dyn_into::<HtmlCanvasElement>().unwrap();
    canvas.set_width(640);
    canvas.set_height(480);
    let ctx = canvas
        .get_context("2d").unwrap().unwrap()
        .dyn_into::<CanvasRenderingContext2d>().unwrap();

    // Store all elements in thread-local
    VIDEO_ELEMENT.with(|v| *v.borrow_mut() = Some(video));
    CANVAS_ELEMENT.with(|c| *c.borrow_mut() = Some(canvas));
    CONTEXT_2D.with(|ctx2d| *ctx2d.borrow_mut() = Some(ctx));
    RESOLUTION.with(|r| *r.borrow_mut() = (640, 480)); // default
}

pub fn get_resolution() -> (u32, u32) {
    RESOLUTION.with(|r| *r.borrow())
}

pub fn get_frame() -> Vec<u8> {
    let (width, height) = get_resolution();

    let data = CONTEXT_2D.with(|ctx_cell| {
        VIDEO_ELEMENT.with(|v_cell| {
            let ctx = ctx_cell.borrow();
            let video = v_cell.borrow();
            if let (Some(ctx), Some(video)) = (ctx.as_ref(), video.as_ref()) {
                ctx.draw_image_with_html_video_element(video, 0.0, 0.0).ok();
                let img_data = ctx.get_image_data(0.0, 0.0, width as f64, height as f64).unwrap();
                Some(img_data.data().to_vec())
            } else {
                None
            }
        })
    });

    match data {
        Some(buffer) => {
            // Convert RGBA (from canvas) to RGB (dropping alpha)
            let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
            for chunk in buffer.chunks(4) {
                rgb.extend_from_slice(&chunk[..3]);
            }
            rgb
        }
        None => vec![0; (width * height * 3) as usize],
    }
}
