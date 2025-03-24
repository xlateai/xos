use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData, MouseEvent};
use std::cell::RefCell;
use std::rc::Rc;

// Virtual Application trait (similar to a virtual class in OOP languages)
pub trait Application {
    fn setup(&mut self, width: u32, height: u32) -> Result<(), JsValue>;
    fn tick(&mut self, width: u32, height: u32) -> Vec<u8>;
    fn on_mouse_down(&mut self, x: f32, y: f32);
}

// Entry point
// We can't use generics with wasm_bindgen, so we'll use a trait object instead
#[wasm_bindgen]
pub fn start_engine() -> Result<(), JsValue> {
    start_with_app(crate::ball_game::BallGame::new())
}

// This function is not exposed to JS but handles the actual engine setup
pub fn start_with_app<T: Application + 'static>(app: T) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    // Get window and document
    let window = web_sys::window().expect("No global window");
    let document = window.document().expect("No document on window");

    // Get canvas and resize it
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

    // Get 2D context
    let context: CanvasRenderingContext2d = canvas
        .get_context("2d")?
        .unwrap()
        .dyn_into()
        .expect("Failed to get 2d context");

    // Create our application (wrapped in Rc<RefCell<>> for shared mutable access)
    let app = Rc::new(RefCell::new(app));
    
    // Call setup
    app.borrow_mut().setup(width, height)?;

    // Set up click handler
    {
        let app_clone = app.clone();
        let click_callback = Closure::wrap(Box::new(move |event: MouseEvent| {
            let x = event.offset_x() as f32;
            let y = event.offset_y() as f32;
            app_clone.borrow_mut().on_mouse_down(x, y);
        }) as Box<dyn FnMut(MouseEvent)>);
        
        canvas.add_event_listener_with_callback(
            "click",
            click_callback.as_ref().unchecked_ref(),
        )?;
        click_callback.forget(); // Leak the closure to keep it alive
    }

    // Start animation loop
    {
        let app_clone = app.clone();
        let canvas_clone = canvas.clone();
        let context_clone = context.clone();

        let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
        let g = f.clone();

        *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
            let width = canvas_clone.width();
            let height = canvas_clone.height();
            
            // Update app and get pixel data
            let pixels = app_clone.borrow_mut().tick(width, height);
            
            // Render to canvas
            let data = wasm_bindgen::Clamped(&pixels[..]);
            let image_data = ImageData::new_with_u8_clamped_array_and_sh(data, width, height)
                .expect("Failed to create ImageData");
            context_clone.put_image_data(&image_data, 0.0, 0.0).expect("put_image_data failed");

            // Request next frame
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

    Ok(())
}