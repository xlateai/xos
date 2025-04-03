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
    panic!("get_frame not implemented yet — will require a canvas copy from video element")
}
