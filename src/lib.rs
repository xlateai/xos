pub mod engine;
pub mod ball_game;

use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    crate::engine::start_web()
}