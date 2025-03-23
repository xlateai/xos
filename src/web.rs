use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};
use std::cell::RefCell;
use std::rc::Rc;

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    let window = web_sys::window().expect("No global window");
    let document = window.document().expect("No document on window");

    let canvas: HtmlCanvasElement = document
        .get_element_by_id("xos-canvas")
        .expect("No canvas with id 'xos-canvas'")
        .dyn_into()
        .expect("Element is not a canvas");

    // Match canvas size to viewport
    let width = window.inner_width()?.as_f64().unwrap() as u32;
    let height = window.inner_height()?.as_f64().unwrap() as u32;
    canvas.set_width(width);
    canvas.set_height(height);

    let context: CanvasRenderingContext2d = canvas
        .get_context("2d")?
        .unwrap()
        .dyn_into()
        .expect("Failed to get 2d context");

    let state = BallState::new(width, height, 30.0);
    animate(canvas, context, state);

    Ok(())
}

fn animate(canvas: HtmlCanvasElement, context: CanvasRenderingContext2d, mut state: BallState) {
    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();

    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        let width = canvas.width();
        let height = canvas.height();
        let mut pixels = vec![0u8; (width * height * 4) as usize];

        state.update(width as f32, height as f32);
        draw_circle(&mut pixels, width, height, state.x, state.y, state.radius);

        let data = wasm_bindgen::Clamped(&pixels[..]);
        let image_data = ImageData::new_with_u8_clamped_array_and_sh(data, width, height)
            .expect("Failed to create ImageData");
        context.put_image_data(&image_data, 0.0, 0.0).expect("put_image_data failed");

        web_sys::window()
            .unwrap()
            .request_animation_frame(f.borrow().as_ref().unwrap().as_ref().unchecked_ref())
            .expect("requestAnimationFrame failed");
    }) as Box<dyn FnMut()>));

    web_sys::window()
        .unwrap()
        .request_animation_frame(g.borrow().as_ref().unwrap().as_ref().unchecked_ref())
        .expect("Initial requestAnimationFrame failed");
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

// -------------------------------------
// Ball Physics State
// -------------------------------------

struct BallState {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    radius: f32,
}

impl BallState {
    fn new(width: u32, height: u32, radius: f32) -> Self {
        Self {
            x: width as f32 / 2.0,
            y: height as f32 / 2.0,
            vx: 1.5,
            vy: 1.0,
            radius,
        }
    }

    fn update(&mut self, width: f32, height: f32) {
        self.x += self.vx;
        self.y += self.vy;

        if self.x - self.radius < 0.0 || self.x + self.radius > width {
            self.vx *= -1.0;
        }
        if self.y - self.radius < 0.0 || self.y + self.radius > height {
            self.vy *= -1.0;
        }
    }
}
