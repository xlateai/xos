use std::cell::RefCell;
use wasm_bindgen::{prelude::*, JsCast};
use web_sys::{
    Document, HtmlCanvasElement, HtmlVideoElement, MediaDevices, MediaStream,
    MediaStreamConstraints, Navigator, Window, CanvasRenderingContext2d, ImageData,
};
use js_sys::{Object, Reflect};
use wasm_bindgen::JsValue;

thread_local! {
    static VIDEO_ELEMENT: RefCell<Option<HtmlVideoElement>> = RefCell::new(None);
    static VIDEO_RESOLUTION: RefCell<(u32, u32)> = RefCell::new((640, 480)); // default fallback
    static CANVAS_CONTEXT: RefCell<Option<CanvasRenderingContext2d>> = RefCell::new(None);
}

pub fn init_camera() {
    let window: Window = web_sys::window().expect("no global window");
    let navigator: Navigator = window.navigator();
    let media_devices: MediaDevices = navigator
        .media_devices()
        .expect("mediaDevices not supported");

    let document: Document = window.document().expect("should have a document");

    // ✅ Setup video element
    let video_element: HtmlVideoElement = document
        .create_element("video")
        .unwrap()
        .dyn_into()
        .unwrap();
    video_element.set_autoplay(true);
    video_element.set_muted(true);
    video_element.set_attribute("playsinline", "").unwrap();

    // ✅ Add to DOM (invisible if needed)
    video_element.style().set_property("display", "none").unwrap();
    document.body().unwrap().append_child(&video_element).unwrap();

    // ✅ Get canvas context
    let canvas: HtmlCanvasElement = document
        .get_element_by_id("xos-canvas")
        .expect("No canvas with id 'xos-canvas'")
        .dyn_into()
        .expect("Element is not a canvas");

    // Manually construct JS options: { willReadFrequently: true }
    let context_options = Object::new();
    Reflect::set(
        &context_options,
        &JsValue::from_str("willReadFrequently"),
        &JsValue::TRUE,
    )
    .expect("Failed to set willReadFrequently");

    // Get the 2d context with the option
    let context = canvas
        .get_context_with_context_options("2d", &context_options)
        .expect("get_context_with_context_options failed")
        .expect("context is null")
        .dyn_into::<CanvasRenderingContext2d>()
        .expect("context is not a 2d context");

    CANVAS_CONTEXT.with(|ctx| *ctx.borrow_mut() = Some(context));
    VIDEO_ELEMENT.with(|v| *v.borrow_mut() = Some(video_element.clone()));

    // ✅ Handle success case
    let success_closure = Closure::wrap(Box::new(move |stream: JsValue| {
        let media_stream = stream.dyn_into::<MediaStream>().unwrap();
        video_element.set_src_object(Some(&media_stream));
    }) as Box<dyn FnMut(JsValue)>);

    let constraints = MediaStreamConstraints::new();
    constraints.set_video(&JsValue::TRUE);

    let _ = media_devices
        .get_user_media_with_constraints(&constraints)
        .unwrap()
        .then(&success_closure);

    success_closure.forget();
}

pub fn get_resolution() -> (u32, u32) {
    VIDEO_ELEMENT.with(|v| {
        if let Some(video) = &*v.borrow() {
            let width = video.video_width();
            let height = video.video_height();
            if width > 0 && height > 0 {
                VIDEO_RESOLUTION.with(|res| *res.borrow_mut() = (width, height));
                return (width, height);
            }
        }
        VIDEO_RESOLUTION.with(|res| *res.borrow())
    })
}

pub fn get_frame() -> Vec<u8> {
    let (width, height) = get_resolution();

    CANVAS_CONTEXT.with(|ctx| {
        VIDEO_ELEMENT.with(|video_opt| {
            if let (Some(context), Some(video)) = (&*ctx.borrow(), &*video_opt.borrow()) {
                let canvas = context.canvas().expect("Context has no canvas");

                // Ensure canvas dimensions match the actual video dimensions
                if canvas.width() != width || canvas.height() != height {
                    canvas.set_width(width);
                    canvas.set_height(height);
                }

                // Draw the current video frame to the (correctly sized) canvas
                context
                    .draw_image_with_html_video_element(video, 0.0, 0.0)
                    .expect("Failed to draw video to canvas");

                // Read pixel data
                let image_data: ImageData = context
                    .get_image_data(0.0, 0.0, width as f64, height as f64)
                    .expect("Failed to get image data");

                image_data.data().to_vec()
            } else {
                vec![]
            }
        })
    })
}

