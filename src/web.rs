use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

// Constants for the canvas
const WIDTH: u32 = 256;
const HEIGHT: u32 = 256;

#[wasm_bindgen(start)]
pub fn run() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();

    let canvas = document
        .get_element_by_id("xos-canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()?;

    let context = canvas
        .get_context("2d")?
        .unwrap()
        .dyn_into::<CanvasRenderingContext2d>()?;

    // Black background
    context.set_fill_style(&"#000".into());
    context.fill_rect(0.0, 0.0, WIDTH as f64, HEIGHT as f64);

    // Draw green circle
    let cx = WIDTH as f64 / 2.0;
    let cy = HEIGHT as f64 / 2.0;
    let radius = 20.0;

    context.begin_path();
    context
        .arc(cx, cy, radius, 0.0, std::f64::consts::PI * 2.0)
        .unwrap();
    context.set_fill_style(&"#00ff00".into());
    context.fill();

    Ok(())
}
