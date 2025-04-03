use nokhwa::{
    pixel_format::RgbFormat,
    utils::{CameraIndex, RequestedFormat, RequestedFormatType},
    Camera,
};

use std::cell::RefCell;

thread_local! {
    static CAMERA: RefCell<Option<Camera>> = RefCell::new(None);
}

/// Initializes the camera (must be called before using `get_frame` or `get_resolution`)
pub fn init_camera() {
    CAMERA.with(|cell| {
        let index = CameraIndex::Index(0);
        let requested =
            RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestResolution);
        let mut cam = Camera::new(index, requested).expect("Failed to open camera");
        cam.open_stream().expect("Failed to open stream");
        *cell.borrow_mut() = Some(cam);
    });
}

/// Gets the current camera resolution
pub fn get_resolution() -> (u32, u32) {
    CAMERA.with(|cell| {
        let cam = cell.borrow();
        let cam = cam.as_ref().expect("Camera not initialized");
        let res = cam.resolution();
        (res.width_x, res.height_y)
    })
}

/// Captures the latest frame from the camera
pub fn get_frame() -> Vec<u8> {
    CAMERA.with(|cell| {
        let mut cam = cell.borrow_mut();
        let cam = cam.as_mut().expect("Camera not initialized");
        match cam.frame() {
            Ok(frame) => frame.decode_image::<RgbFormat>().unwrap().into_vec(),
            Err(_) => {
                let res = cam.resolution();
                let (width, height) = (res.width_x, res.height_y);
                vec![0; (width * height * 3) as usize]
            }
        }
    })
}
