use std::cell::RefCell;
use wasm_bindgen::{prelude::*, JsCast};
use web_sys::{
    Document, HtmlCanvasElement, HtmlVideoElement, MediaDevices, MediaStream,
    MediaStreamConstraints, Navigator, Window,
};

thread_local! {
    static VIDEO_ELEMENT: RefCell<Option<HtmlVideoElement>> = RefCell::new(None);
    static VIDEO_RESOLUTION: RefCell<(u32, u32)> = RefCell::new((640, 480)); // default fallback
}

pub fn init_camera() {
    let window: Window = web_sys::window().expect("no global window");
    let navigator: Navigator = window.navigator();
    let media_devices: MediaDevices = navigator
        .media_devices()
        .expect("mediaDevices not supported");

    let document: Document = window.document().expect("should have a document");
    let video_element: HtmlVideoElement = document
        .create_element("video")
        .unwrap()
        .dyn_into()
        .unwrap();
    video_element.set_autoplay(true);
    video_element.set_muted(true);
    video_element.set_attribute("playsinline", "").unwrap(); // Required for iOS

    let mut constraints = MediaStreamConstraints::new();
    constraints.video(&JsValue::TRUE);

    let video_clone = video_element.clone();
    let success_closure = Closure::wrap(Box::new(move |stream: JsValue| {
        let media_stream = stream.dyn_into::<MediaStream>().unwrap();
        video_clone.set_src_object(Some(&media_stream));
    }) as Box<dyn FnMut(JsValue)>);

    media_devices
        .get_user_media_with_constraints(&constraints)
        .unwrap()
        .then(&success_closure);

    success_closure.forget(); // Prevent dropping

    VIDEO_ELEMENT.with(|v| *v.borrow_mut() = Some(video_element));
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
    use wasm_bindgen::Clamped;
    use web_sys::{ImageData, HtmlCanvasElement, CanvasRenderingContext2d};

    VIDEO_ELEMENT.with(|video_ref| {
        let video = video_ref.borrow();
        let video = video.as_ref().expect("Video not initialized");

        let width = video.video_width();
        let height = video.video_height();

        // Early-out if the video hasn't loaded yet
        if width == 0 || height == 0 {
            return vec![0; (640 * 480 * 3) as usize]; // black placeholder frame
        }

        let document = web_sys::window().unwrap().document().unwrap();
        let canvas = document
            .create_element("canvas").unwrap()
            .dyn_into::<HtmlCanvasElement>().unwrap();
        canvas.set_width(width);
        canvas.set_height(height);

        let context = canvas
            .get_context("2d").unwrap()
            .unwrap()
            .dyn_into::<CanvasRenderingContext2d>().unwrap();

        context.draw_image_with_html_video_element(video, 0.0, 0.0).unwrap();

        let image_data: ImageData = context
            .get_image_data(0.0, 0.0, width.into(), height.into())
            .expect("get_image_data failed");

        let raw = image_data.data(); // This is a Clamped<Vec<u8>> in RGBA format

        // Convert RGBA to RGB
        let mut rgb = Vec::with_capacity((width * height * 3) as usize);
        for chunk in raw.iter().collect::<Vec<_>>().chunks(4) {
            rgb.push(*chunk[0]);
            rgb.push(*chunk[1]);
            rgb.push(*chunk[2]);
        }

        rgb
    })
}


