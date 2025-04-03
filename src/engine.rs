use std::cell::RefCell;

#[cfg(not(target_arch = "wasm32"))]
use pixels::{Pixels, SurfaceTexture};
#[cfg(not(target_arch = "wasm32"))]
use winit::{
    event::*,
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

/// Trait that all XOS apps must implement
pub trait Application {
    fn setup(&mut self, width: u32, height: u32) -> Result<(), String>;
    fn tick(&mut self, width: u32, height: u32) -> Vec<u8>;

    /// Events
    fn on_mouse_down(&mut self, _x: f32, _y: f32) {}
    fn on_mouse_up(&mut self, _x: f32, _y: f32) {}
    fn on_mouse_move(&mut self, _x: f32, _y: f32) {}

    /// Called by engine during tick for stable polling
    fn mouse_position(&self) -> Option<(f32, f32)> {
        None
    }
}

/// Shared function to validate frame buffer size
fn validate_frame_dimensions(label: &str, width: u32, height: u32, buffer: &[u8]) {
    let expected = (width * height * 4) as usize;
    let actual = buffer.len();
    if expected != actual {
        #[cfg(target_arch = "wasm32")]
        web_sys::console::error_1(&format!(
            "[{label}] Frame size mismatch: expected {} ({}x{}x4), got {}",
            expected, width, height, actual
        ).into());

        #[cfg(not(target_arch = "wasm32"))]
        eprintln!(
            "[{label}] Frame size mismatch: expected {} ({}x{}x4), got {}",
            expected, width, height, actual
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct MouseTrackedApp {
    app: Box<dyn Application>,
    cursor: std::rc::Rc<RefCell<(f32, f32)>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Application for MouseTrackedApp {
    fn setup(&mut self, w: u32, h: u32) -> Result<(), String> {
        self.app.setup(w, h)
    }

    fn tick(&mut self, w: u32, h: u32) -> Vec<u8> {
        self.app.tick(w, h)
    }

    fn on_mouse_down(&mut self, x: f32, y: f32) {
        self.app.on_mouse_down(x, y);
    }

    fn on_mouse_up(&mut self, x: f32, y: f32) {
        self.app.on_mouse_up(x, y);
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        *self.cursor.borrow_mut() = (x, y);
        self.app.on_mouse_move(x, y);
    }

    fn mouse_position(&self) -> Option<(f32, f32)> {
        Some(*self.cursor.borrow())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn start_native(app: Box<dyn Application>) -> Result<(), Box<dyn std::error::Error>> {
    use std::rc::Rc;

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("XOS Game")
        .build(&event_loop)?;

    let mut size = window.inner_size();
    let surface_texture = SurfaceTexture::new(size.width, size.height, &window);
    let mut pixels = Pixels::new(size.width, size.height, surface_texture)?;

    let cursor_position = Rc::new(RefCell::new((0.0_f32, 0.0_f32)));
    let mut app = Box::new(MouseTrackedApp {
        app,
        cursor: cursor_position.clone(),
    });

    app.setup(size.width, size.height)?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        match event {
            Event::RedrawRequested(_) => {
                let current_size = window.inner_size();

                if current_size != size {
                    size = current_size;
                    let _ = pixels.resize_surface(size.width, size.height);
                    let _ = pixels.resize_buffer(size.width, size.height);
                }

                let frame = pixels.frame_mut();
                let buffer = app.tick(size.width, size.height);
                validate_frame_dimensions("native tick", size.width, size.height, &buffer);
                frame.copy_from_slice(&buffer);
                let _ = pixels.render();
            }

            Event::MainEventsCleared => {
                window.request_redraw();
            }

            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,

                WindowEvent::Resized(new_size) => {
                    size = new_size;
                    let _ = pixels.resize_surface(size.width, size.height);
                    let _ = pixels.resize_buffer(size.width, size.height);
                    window.request_redraw();
                },

                WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                    size = *new_inner_size;
                    let _ = pixels.resize_surface(size.width, size.height);
                    let _ = pixels.resize_buffer(size.width, size.height);
                    window.request_redraw();
                },

                WindowEvent::CursorMoved { position, .. } => {
                    let pos = (position.x as f32, position.y as f32);
                    *cursor_position.borrow_mut() = pos;
                    app.on_mouse_move(pos.0, pos.1);
                },

                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Left,
                    ..
                } => {
                    let (x, y) = *cursor_position.borrow();
                    app.on_mouse_down(x, y);
                },

                WindowEvent::MouseInput {
                    state: ElementState::Released,
                    button: MouseButton::Left,
                    ..
                } => {
                    let (x, y) = *cursor_position.borrow();
                    app.on_mouse_up(x, y);
                },

                _ => {}
            },
            _ => {}
        }
    });
}

#[cfg(target_arch = "wasm32")]
pub fn run_web(app: Box<dyn Application>) -> Result<(), JsValue> {
    use std::rc::Rc;
    use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData, MouseEvent};

    console_error_panic_hook::set_once();

    let window = web_sys::window().expect("no global window exists");
    let document = window.document().expect("should have a document");

    let canvas: HtmlCanvasElement = document
        .get_element_by_id("xos-canvas")
        .expect("No canvas with id 'xos-canvas'")
        .dyn_into()
        .expect("Element is not a canvas");

    let width = window.inner_width()?.as_f64().unwrap() as u32;
    let height = window.inner_height()?.as_f64().unwrap() as u32;
    canvas.set_width(width);
    canvas.set_height(height);

    let context: CanvasRenderingContext2d = canvas
        .get_context("2d")?
        .unwrap()
        .dyn_into()
        .expect("Failed to get 2d context");

    let app = Rc::new(RefCell::new(app));
    app.borrow_mut().setup(width, height).map_err(|e| JsValue::from_str(&e))?;

    {
        let app_clone = app.clone();
        let move_callback = Closure::wrap(Box::new(move |event: MouseEvent| {
            let x = event.offset_x() as f32;
            let y = event.offset_y() as f32;
            app_clone.borrow_mut().on_mouse_move(x, y);
        }) as Box<dyn FnMut(MouseEvent)>);
        canvas.add_event_listener_with_callback("mousemove", move_callback.as_ref().unchecked_ref())?;
        move_callback.forget();
    }

    {
        let app_clone = app.clone();
        let down_callback = Closure::wrap(Box::new(move |event: MouseEvent| {
            let x = event.offset_x() as f32;
            let y = event.offset_y() as f32;
            app_clone.borrow_mut().on_mouse_down(x, y);
        }) as Box<dyn FnMut(MouseEvent)>);
        canvas.add_event_listener_with_callback("mousedown", down_callback.as_ref().unchecked_ref())?;
        down_callback.forget();
    }

    {
        let app_clone = app.clone();
        let up_callback = Closure::wrap(Box::new(move |event: MouseEvent| {
            let x = event.offset_x() as f32;
            let y = event.offset_y() as f32;
            app_clone.borrow_mut().on_mouse_up(x, y);
        }) as Box<dyn FnMut(MouseEvent)>);
        canvas.add_event_listener_with_callback("mouseup", up_callback.as_ref().unchecked_ref())?;
        up_callback.forget();
    }

    {
        let app_clone = app.clone();
        let canvas_clone = canvas.clone();
        let context_clone = context.clone();

        let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
        let g = f.clone();

        *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
            let width = canvas_clone.width();
            let height = canvas_clone.height();
            let pixels = app_clone.borrow_mut().tick(width, height);

            validate_frame_dimensions("wasm tick", width, height, &pixels);

            let data = wasm_bindgen::Clamped(&pixels[..]);
            let image_data = ImageData::new_with_u8_clamped_array_and_sh(data, width, height)
                .expect("Failed to create ImageData");
            context_clone.put_image_data(&image_data, 0.0, 0.0).expect("put_image_data failed");

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