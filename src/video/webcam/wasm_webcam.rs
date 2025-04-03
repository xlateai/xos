use std::cell::RefCell;
use wasm_bindgen::{prelude::*, JsCast};
use web_sys::{
    Document, HtmlCanvasElement, HtmlVideoElement, MediaDevices, MediaStream,
    MediaStreamConstraints, Navigator, Window, CanvasRenderingContext2d, ImageData,
    console,
};
use js_sys::{Object, Reflect};
use wasm_bindgen::JsValue;

thread_local! {
    static VIDEO_ELEMENT: RefCell<Option<HtmlVideoElement>> = RefCell::new(None);
    static VIDEO_RESOLUTION: RefCell<(u32, u32)> = RefCell::new((640, 480)); // default fallback
    static CANVAS_CONTEXT: RefCell<Option<CanvasRenderingContext2d>> = RefCell::new(None);
}

pub fn init_camera() {
    console::log_1(&"Initializing camera...".into());
    
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
    
    console::log_1(&"Video element created".into());
    
    video_element.set_autoplay(true);
    video_element.set_muted(true);
    video_element.set_attribute("playsinline", "").unwrap();
    
    // Make video slightly visible for debugging
    video_element.style().set_property("position", "fixed").unwrap();
    video_element.style().set_property("top", "0").unwrap();
    video_element.style().set_property("right", "0").unwrap();
    video_element.style().set_property("width", "160px").unwrap();
    video_element.style().set_property("height", "120px").unwrap();
    video_element.style().set_property("opacity", "0.3").unwrap();
    video_element.style().set_property("z-index", "9999").unwrap();
    
    document.body().unwrap().append_child(&video_element).unwrap();

    // ✅ Get canvas context
    let canvas: HtmlCanvasElement = match document.get_element_by_id("xos-canvas") {
        Some(element) => element.dyn_into().expect("Element is not a canvas"),
        None => {
            console::error_1(&"No canvas with id 'xos-canvas', creating one for debugging".into());
            let canvas = document
                .create_element("canvas")
                .unwrap()
                .dyn_into::<HtmlCanvasElement>()
                .unwrap();
            canvas.set_id("xos-canvas");
            canvas.style().set_property("position", "fixed").unwrap();
            canvas.style().set_property("top", "130px").unwrap();
            canvas.style().set_property("right", "0").unwrap();
            canvas.style().set_property("width", "160px").unwrap();
            canvas.style().set_property("height", "120px").unwrap();
            canvas.style().set_property("border", "1px solid red").unwrap();
            canvas.style().set_property("z-index", "9999").unwrap();
            document.body().unwrap().append_child(&canvas).unwrap();
            canvas
        }
    };

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

    // ✅ Setup constraints
    let video_constraints = Object::new();
    Reflect::set(
        &video_constraints, 
        &JsValue::from_str("width"), 
        &JsValue::from(1280)
    ).unwrap();
    Reflect::set(
        &video_constraints, 
        &JsValue::from_str("height"), 
        &JsValue::from(720)
    ).unwrap();
    
    let constraints = MediaStreamConstraints::new();
    constraints.set_video(&video_constraints);

    // ✅ Handle success case
    let success_callback = Closure::once(Box::new(|stream: JsValue| {
        console::log_1(&"Got media stream, setting up video".into());
        
        let media_stream = stream.dyn_into::<MediaStream>().unwrap();
        
        VIDEO_ELEMENT.with(|v| {
            if let Some(video) = &*v.borrow() {
                video.set_src_object(Some(&media_stream));
                
                // Setup metadata loaded event
                let metadata_callback = Closure::once(Box::new(move || {
                    VIDEO_ELEMENT.with(|v| {
                        if let Some(video) = &*v.borrow() {
                            let width = video.video_width();
                            let height = video.video_height();
                            
                            console::log_3(
                                &"Video dimensions:".into(),
                                &width.into(),
                                &height.into()
                            );
                            
                            if width > 0 && height > 0 {
                                VIDEO_RESOLUTION.with(|res| *res.borrow_mut() = (width, height));
                            } else {
                                console::error_1(&"Video has invalid dimensions".into());
                            }
                        }
                    });
                }) as Box<dyn FnOnce()>);
                
                video.set_onloadedmetadata(Some(metadata_callback.as_ref().unchecked_ref()));
                metadata_callback.forget();
            }
        });
    }) as Box<dyn FnOnce(JsValue)>);

    // Create error handler
    let error_callback = Closure::once(Box::new(move |err: JsValue| {
        console::error_1(&format!("Error getting user media: {:?}", err).into());
    }) as Box<dyn FnOnce(JsValue)>);

    // Request camera access
    let promise = media_devices
        .get_user_media_with_constraints(&constraints)
        .expect("Failed to get user media");
    
    let _ = promise.then(&success_callback).catch(&error_callback);
    
    success_callback.forget();
    error_callback.forget();
}

pub fn get_resolution() -> (u32, u32) {
    let resolution = VIDEO_ELEMENT.with(|v| {
        if let Some(video) = &*v.borrow() {
            let width = video.video_width();
            let height = video.video_height();
            if width > 0 && height > 0 {
                VIDEO_RESOLUTION.with(|res| *res.borrow_mut() = (width, height));
                (width, height)
            } else {
                VIDEO_RESOLUTION.with(|res| *res.borrow())
            }
        } else {
            VIDEO_RESOLUTION.with(|res| *res.borrow())
        }
    });
    
    console::log_3(
        &"Current resolution:".into(),
        &resolution.0.into(),
        &resolution.1.into(),
    );
    
    resolution
}

pub fn get_frame() -> Vec<u8> {
    let (width, height) = get_resolution();
    
    if width == 0 || height == 0 {
        console::error_1(&"Invalid dimensions for frame capture".into());
        return Vec::new();
    }
    
    let result = CANVAS_CONTEXT.with(|ctx| {
        VIDEO_ELEMENT.with(|video_opt| {
            if let (Some(context), Some(video)) = (&*ctx.borrow(), &*video_opt.borrow()) {
                let canvas = context.canvas().expect("Context has no canvas");

                // Ensure canvas dimensions match the actual video dimensions
                if canvas.width() != width || canvas.height() != height {
                    console::log_3(
                        &"Resizing canvas to match video:".into(),
                        &width.into(),
                        &height.into()
                    );
                    canvas.set_width(width);
                    canvas.set_height(height);
                }

                // Check if video is actually playing and has valid content
                if video.ready_state() < 2 {  // HAVE_CURRENT_DATA = 2
                    console::warn_1(&format!("Video not ready, state: {}", video.ready_state()).into());
                    return Vec::new();
                }

                // Clear canvas before drawing
                context.clear_rect(0.0, 0.0, width as f64, height as f64);
                
                // Draw the current video frame to the canvas
                if let Err(e) = context.draw_image_with_html_video_element(video, 0.0, 0.0) {
                    console::error_2(&"Failed to draw video to canvas:".into(), &e);
                    return Vec::new();
                }

                // Read pixel data
                let image_data = match context.get_image_data(0.0, 0.0, width as f64, height as f64) {
                    Ok(data) => data,
                    Err(e) => {
                        console::error_2(&"Failed to get image data:".into(), &e);
                        return Vec::new();
                    }
                };

                let data = image_data.data();
                let data_vec = data.to_vec();
                
                // Debug info: Calculate average RGB values and check first few pixels
                let data_len = data.len() as usize;
                if data_len > 0 {
                    let total_pixels = data_len / 4;
                    
                    // Calculate averages
                    let mut sum_r = 0;
                    let mut sum_g = 0;
                    let mut sum_b = 0;
                    
                    let sample_count = total_pixels.min(1000);  // Sample up to 1000 pixels for performance
                    for i in 0..sample_count {
                        sum_r += data[i * 4] as u32;
                        sum_g += data[i * 4 + 1] as u32;
                        sum_b += data[i * 4 + 2] as u32;
                    }
                    
                    let samples = sample_count as u32;
                    let avg_r = if samples > 0 { sum_r / samples } else { 0 };
                    let avg_g = if samples > 0 { sum_g / samples } else { 0 };
                    let avg_b = if samples > 0 { sum_b / samples } else { 0 };
                    
                    console::log_4(
                        &"Frame stats:".into(),
                        &format!("Size: {} bytes", data_len).into(),
                        &format!("First pixel: [{},{},{}]", 
                                data[0], 
                                data[1], 
                                data[2]).into(),
                        &format!("Avg RGB: [{},{},{}]", avg_r, avg_g, avg_b).into(),
                    );
                    
                    // Check if frame seems to be all black or all one color
                    if avg_r < 5 && avg_g < 5 && avg_b < 5 {
                        console::warn_1(&"Frame appears to be black or very dark".into());
                    } else if (avg_r as i32 - avg_g as i32).abs() < 3 && 
                              (avg_r as i32 - avg_b as i32).abs() < 3 && 
                              (avg_g as i32 - avg_b as i32).abs() < 3 {
                        console::warn_1(&"Frame appears to be grayscale or single color".into());
                    }
                } else {
                    console::error_1(&"Empty image data".into());
                }
                
                data_vec
            } else {
                console::error_1(&"Missing context or video element".into());
                Vec::new()
            }
        })
    });
    
    result
}

// New debug function to ensure we can view the current frame
#[wasm_bindgen]
pub fn debug_show_frame() {
    console::log_1(&"Attempting to show debug frame on canvas".into());
    let _ = get_frame();  // This will draw to the canvas and log diagnostics
    console::log_1(&"Debug frame shown on canvas".into());
}

// New function to check video status
#[wasm_bindgen]
pub fn get_video_status() -> JsValue {
    let status = VIDEO_ELEMENT.with(|v| {
        if let Some(video) = &*v.borrow() {
            let status = Object::new();
            
            // Basic video properties
            Reflect::set(&status, &"readyState".into(), &video.ready_state().into()).unwrap();
            Reflect::set(&status, &"videoWidth".into(), &video.video_width().into()).unwrap();
            Reflect::set(&status, &"videoHeight".into(), &video.video_height().into()).unwrap();
            Reflect::set(&status, &"paused".into(), &video.paused().into()).unwrap();
            Reflect::set(&status, &"ended".into(), &video.ended().into()).unwrap();
            Reflect::set(&status, &"currentTime".into(), &video.current_time().into()).unwrap();
            Reflect::set(&status, &"duration".into(), &video.duration().into()).unwrap();
            
            // Check if srcObject exists and is a MediaStream
            let src_object = video.src_object();
            if let Some(src) = src_object {
                if src.is_instance_of::<MediaStream>() {
                    let stream: MediaStream = src.dyn_into().unwrap();
                    Reflect::set(&status, &"hasStream".into(), &true.into()).unwrap();
                    Reflect::set(&status, &"streamActive".into(), &stream.active().into()).unwrap();
                    
                    let tracks = stream.get_video_tracks();
                    Reflect::set(&status, &"videoTrackCount".into(), &tracks.length().into()).unwrap();
                    
                    if tracks.length() > 0 {
                        let track = tracks.get(0);
                        Reflect::set(&status, &"trackEnabled".into(), 
                            &js_sys::Reflect::get(&track, &"enabled".into()).unwrap()).unwrap();
                        Reflect::set(&status, &"trackId".into(), 
                            &js_sys::Reflect::get(&track, &"id".into()).unwrap()).unwrap();
                    }
                } else {
                    Reflect::set(&status, &"hasStream".into(), &false.into()).unwrap();
                }
            } else {
                Reflect::set(&status, &"hasStream".into(), &false.into()).unwrap();
            }
            
            status.into()
        } else {
            let error = Object::new();
            Reflect::set(&error, &"error".into(), &"No video element initialized".into()).unwrap();
            error.into()
        }
    });
    
    console::log_2(&"Video status:".into(), &status);
    status
}