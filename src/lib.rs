pub mod engine;
pub mod ball_game;

use wasm_bindgen::prelude::*;

// Re-export start function for the JavaScript/WASM interface
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    // Call the engine's start function with our chosen game
    engine::start_engine()
}