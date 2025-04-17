pub mod ball;
pub mod blank;
pub mod camera;
pub mod whiteboard;
pub mod tracers;
pub mod waveform;
pub mod scroll;
pub mod text;
pub mod wireframe;
pub mod wireframe_text;

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
        "text" => Some(Box::new(text::TextApp::new())),
        "wireframe" => Some(Box::new(wireframe::WireframeDemo::new())),
        "wireframe_text" => Some(Box::new(wireframe_text::WireframeText::new())),
        _ => None,
    }
}
