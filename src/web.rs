use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData, MouseEvent};
use std::cell::RefCell;
use std::rc::Rc;
use js_sys;

// change background color as needed of course
const BACKGROUND_COLOR: (u8, u8, u8) = (64, 0, 64);

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

    // Create a vector of balls instead of a single ball
    let balls = Rc::new(RefCell::new(vec![BallState::new(width, height, 30.0)]));

    // Set up click handler
    {
        let balls_clone = balls.clone();
        let click_callback = Closure::wrap(Box::new(move |event: MouseEvent| {
            let x = event.offset_x() as f32;
            let y = event.offset_y() as f32;
            balls_clone.borrow_mut().push(BallState::new_at_position(x, y, 30.0));
        }) as Box<dyn FnMut(MouseEvent)>);
        
        canvas.add_event_listener_with_callback(
            "click",
            click_callback.as_ref().unchecked_ref(),
        )?;
        click_callback.forget(); // Leak the closure to keep it alive
    }

    animate(canvas, context, balls);

    Ok(())
}

fn animate(canvas: HtmlCanvasElement, context: CanvasRenderingContext2d, balls: Rc<RefCell<Vec<BallState>>>) {
    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();

    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        let width = canvas.width();
        let height = canvas.height();
        let mut pixels = vec![0u8; (width * height * 4) as usize];

        // Fill background first
        for i in (0..pixels.len()).step_by(4) {
            pixels[i + 0] = BACKGROUND_COLOR.0;
            pixels[i + 1] = BACKGROUND_COLOR.1;
            pixels[i + 2] = BACKGROUND_COLOR.2;
            pixels[i + 3] = 0xff;
        }

        // Update and draw all balls
        for ball in balls.borrow_mut().iter_mut() {
            ball.update(width as f32, height as f32);
            draw_circle(&mut pixels, width, height, ball.x, ball.y, ball.radius);
        }

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

    // Calculate bounding box to avoid checking every pixel
    let start_x = (cx - radius).max(0.0) as u32;
    let end_x = (cx + radius + 1.0).min(width as f32) as u32;
    let start_y = (cy - radius).max(0.0) as u32;
    let end_y = (cy + radius + 1.0).min(height as f32) as u32;

    for y in start_y..end_y {
        for x in start_x..end_x {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let distance_squared = dx * dx + dy * dy;

            let i = ((y * width + x) * 4) as usize;

            if distance_squared <= radius_squared {
                pixels[i + 0] = 0x00; // R
                pixels[i + 1] = 0xff; // G
                pixels[i + 2] = 0x00; // B
                pixels[i + 3] = 0xff; // A
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

    fn new_at_position(x: f32, y: f32, radius: f32) -> Self {
        let vx = rand_float(-2.0, 2.0);
        let vy = rand_float(-2.0, 2.0);
        
        Self {
            x,
            y,
            vx,
            vy,
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

// Simple random number generator for velocity
fn rand_float(min: f32, max: f32) -> f32 {
    let random = js_sys::Math::random() as f32;
    min + random * (max - min)
}