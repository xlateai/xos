use std::cell::RefCell;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{HtmlCanvasElement, HtmlVideoElement, MediaStreamConstraints, MediaDevices, Navigator};

thread_local! {
    static VIDEO_ELEMENT: RefCell<Option<HtmlVideoElement>> = RefCell::new(None);
    static CANVAS_ELEMENT: RefCell<Option<HtmlCanvasElement>> = RefCell::new(None);
}

pub fn init_camera() {
    let window = web_sys::window().expect("no global `window` exists");
    let navigator: Navigator = window.navigator();
    let media_devices: MediaDevices = navigator.media_devices().expect("no media devices");

    let constraints = MediaStreamConstraints::new();
    // constraints.video(&JsValue::TRUE);
    constraints.set_video(&JsValue::TRUE);

    let promise = media_devices.get_user_media_with_constraints(&constraints)
        .expect("failed to request getUserMedia");

    let video_promise = wasm_bindgen_futures::JsFuture::from(promise);

    // Spawn async initialization task
    wasm_bindgen_futures::spawn_local(async move {
        let stream = match video_promise.await {
            Ok(media_stream) => media_stream.dyn_into::<web_sys::MediaStream>().unwrap(),
            Err(err) => {
                web_sys::console::error_1(&err);
                return;
            }
        };

        let document = window.document().unwrap();
        let video = document.create_element("video").unwrap().unchecked_into::<HtmlVideoElement>();
        video.set_autoplay(true);
        video.set_attribute("playsinline", "").unwrap(); // required on iOS
        video.set_src_object(Some(&stream));

        let canvas = document.create_element("canvas").unwrap().unchecked_into::<HtmlCanvasElement>();

        VIDEO_ELEMENT.with(|v| *v.borrow_mut() = Some(video));
        CANVAS_ELEMENT.with(|c| *c.borrow_mut() = Some(canvas));
    });
}

pub fn get_resolution() -> (u32, u32) {
    VIDEO_ELEMENT.with(|video_ref| {
        if let Some(video) = &*video_ref.borrow() {
            (video.video_width(), video.video_height())
        } else {
            (0, 0)
        }
    })
}

pub fn get_frame() -> Vec<u8> {
    let mut pixel_data = vec![];

    VIDEO_ELEMENT.with(|video_ref| {
        CANVAS_ELEMENT.with(|canvas_ref| {
            if let (Some(video), Some(canvas)) = (&*video_ref.borrow(), &*canvas_ref.borrow()) {
                let width = video.video_width();
                let height = video.video_height();

                if width == 0 || height == 0 {
                    return;
                }

                canvas.set_width(width);
                canvas.set_height(height);

                let ctx = canvas
                    .get_context("2d").unwrap().unwrap()
                    .dyn_into::<web_sys::CanvasRenderingContext2d>().unwrap();

                ctx.draw_image_with_html_video_element(video, 0.0, 0.0).unwrap();

                let image_data = ctx
                    .get_image_data(0.0, 0.0, width as f64, height as f64)
                    .unwrap()
                    .data();

                // Convert RGBA to RGB
                let raw = image_data.to_vec();
                pixel_data = raw
                    .chunks(4)
                    .flat_map(|chunk| chunk.iter().take(3)) // keep only R, G, B
                    .cloned()
                    .collect();
            }
        });
    });

    pixel_data
}
