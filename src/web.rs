use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

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

    canvas.set_width(WIDTH);
    canvas.set_height(HEIGHT);

    let context = canvas
        .get_context("2d")?
        .unwrap()
        .dyn_into::<CanvasRenderingContext2d>()?;

    // Allocate pixel buffer (RGBA)
    let mut pixels = vec![0u8; (WIDTH * HEIGHT * 4) as usize];

    // Draw a green circle on black background
    draw_circle(&mut pixels, WIDTH, HEIGHT);

    // Convert Vec<u8> to ImageData
    let data = wasm_bindgen::Clamped(&pixels[..]);
    let image_data = ImageData::new_with_u8_clamped_array_and_sh(
        data,
        WIDTH,
        HEIGHT,
    )?;

    // Render to canvas
    context.put_image_data(&image_data, 0.0, 0.0)?;

    Ok(())
}

fn draw_circle(pixels: &mut [u8], width: u32, height: u32) {
    let center_x = width as f32 / 2.0;
    let center_y = height as f32 / 2.0;
    let radius = 20.0;
    let radius_squared = radius * radius;

    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - center_x;
            let dy = y as f32 - center_y;
            let distance_squared = dx * dx + dy * dy;

            let i = ((y * width + x) * 4) as usize;

            if distance_squared <= radius_squared {
                // Green circle
                pixels[i + 0] = 0x00; // R
                pixels[i + 1] = 0xff; // G
                pixels[i + 2] = 0x00; // B
                pixels[i + 3] = 0xff; // A
            } else {
                // Black background
                pixels[i + 0] = 0x00;
                pixels[i + 1] = 0x00;
                pixels[i + 2] = 0x00;
                pixels[i + 3] = 0xff;
            }
        }
    }
}
