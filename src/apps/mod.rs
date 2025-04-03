pub mod ball_game;
pub mod blank;


use crate::engine::Application;

pub fn get_app(name: &str) -> Option<Box<dyn Application>> {
    match name {
        "ball" => Some(Box::new(ball_game::BallGame::new())),
        "blank" => Some(Box::new(blank::BlankApp::new())),
        _ => None,
    }
}
