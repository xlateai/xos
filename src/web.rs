use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

const WIDTH: u32 = 256;
const HEIGHT: u32 = 256;
const RADIUS: f32 = 20.0;

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
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

    let state = BallState::new(WIDTH, HEIGHT, RADIUS);
    animate(context, state);

    Ok(())
}

fn animate(context: CanvasRenderingContext2d, mut state: BallState) {
    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();

    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        // Create new pixel buffer
        let mut pixels = vec![0u8; (WIDTH * HEIGHT * 4) as usize];

        // Update ball position
        state.update();

        // Draw the ball
        draw_circle(&mut pixels, WIDTH, HEIGHT, state.x, state.y, RADIUS);

        // Convert to ImageData and blit
        let data = wasm_bindgen::Clamped(&pixels[..]);
        let image_data =
            ImageData::new_with_u8_clamped_array_and_sh(data, WIDTH, HEIGHT).unwrap();
        context.put_image_data(&image_data, 0.0, 0.0).unwrap();

        // Schedule next frame
        web_sys::window()
            .unwrap()
            .request_animation_frame(f.borrow().as_ref().unwrap().as_ref().unchecked_ref())
            .unwrap();
    }) as Box<dyn FnMut()>));

    // Start the loop
    web_sys::window()
        .unwrap()
        .request_animation_frame(g.borrow().as_ref().unwrap().as_ref().unchecked_ref())
        .unwrap();
}

fn draw_circle(pixels: &mut [u8], width: u32, height: u32, cx: f32, cy: f32, radius: f32) {
    let radius_squared = radius * radius;

    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let distance_squared = dx * dx + dy * dy;

            let i = ((y * width + x) * 4) as usize;

            if distance_squared <= radius_squared {
                pixels[i + 0] = 0x00; // R
                pixels[i + 1] = 0xff; // G
                pixels[i + 2] = 0x00; // B
                pixels[i + 3] = 0xff; // A
            } else {
                pixels[i + 0] = 0x00;
                pixels[i + 1] = 0x00;
                pixels[i + 2] = 0x00;
                pixels[i + 3] = 0xff;
            }
        }
    }
}

use std::cell::RefCell;
use std::rc::Rc;

// Ball physics state
struct BallState {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    width: f32,
    height: f32,
    radius: f32,
}

impl BallState {
    fn new(width: u32, height: u32, radius: f32) -> Self {
        Self {
            x: width as f32 / 2.0,
            y: height as f32 / 2.0,
            vx: 1.5,
            vy: 1.0,
            width: width as f32,
            height: height as f32,
            radius,
        }
    }

    fn update(&mut self) {
        self.x += self.vx;
        self.y += self.vy;

        if self.x - self.radius < 0.0 || self.x + self.radius > self.width {
            self.vx *= -1.0;
        }
        if self.y - self.radius < 0.0 || self.y + self.radius > self.height {
            self.vy *= -1.0;
        }
    }
}
