pub mod ball;
pub mod blank;
pub mod camera;
pub mod whiteboard;
pub mod tracers;
pub mod waveform;
pub mod scroll;

use crate::engine::Application;

pub fn get_app(name: &str) -> Option<Box<dyn Application>> {
    match name {
        "ball" => Some(Box::new(ball::BallGame::new())),
        "tracers" => Some(Box::new(tracers::TracersApp::new())),
        "blank" => Some(Box::new(blank::BlankApp::new())),
        "camera" => Some(Box::new(camera::CameraApp::new())),
        "whiteboard" => Some(Box::new(whiteboard::Whiteboard::new())),
        "waveform" => Some(Box::new(waveform::Waveform::new())),
        "scroll" => Some(Box::new(scroll::ScrollApp::new())),
        _ => None,
    }
}
