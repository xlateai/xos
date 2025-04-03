pub mod ball;
pub mod blank;
pub mod camera;
pub mod whiteboard;

use crate::engine::Application;

pub fn get_app(name: &str) -> Option<Box<dyn Application>> {
    match name {
        "ball" => Some(Box::new(ball::BallGame::new())),
        "blank" => Some(Box::new(blank::BlankApp::new())),
        "camera" => Some(Box::new(camera::CameraApp::new())),
        "whiteboard" => Some(Box::new(whiteboard::Whiteboard::new())),
        _ => None,
    }
}
