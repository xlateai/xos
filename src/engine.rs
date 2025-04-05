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


#[derive(Debug, Clone)]
pub struct EngineState {
    pub frame: FrameState,
    pub mouse: MouseState,
}

#[derive(Debug, Clone)]
pub struct FrameState {
    pub width: u32,
    pub height: u32,
    pub buffer: RefCell<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct MouseState {
    pub x: f32,
    pub y: f32,
    pub is_down: bool,
}


/// Trait that all XOS apps must implement
pub trait Application {
    fn setup(&mut self, state: &EngineState) -> Result<(), String>;
    fn tick(&mut self, state: &EngineState);

    fn on_mouse_down(&mut self, _x: f32, _y: f32) {}
    fn on_mouse_up(&mut self, _x: f32, _y: f32) {}
    fn on_mouse_move(&mut self, _x: f32, _y: f32) {}

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
    fn setup(&mut self, state: &EngineState) -> Result<(), String> {
        self.app.setup(state)
    }
    
    fn tick(&mut self, state: &EngineState) {
        self.app.tick(state)
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

    let engine_state = Rc::new(RefCell::new(EngineState {
        frame: FrameState {
            width: size.width,
            height: size.height,
            buffer: RefCell::new(vec![0; (size.width * size.height * 4) as usize]),
        },
        mouse: MouseState {
            x: 0.0,
            y: 0.0,
            is_down: false,
        },
    }));

    let mut app = app;

    app.setup(&engine_state.borrow())?;

    // let engine_state_clone = engine_state.clone();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        match event {
            Event::RedrawRequested(_) => {
                let current_size = window.inner_size();
            
                // Handle window resizing (and buffer reallocation)
                if current_size != size {
                    size = current_size;
                    let _ = pixels.resize_surface(size.width, size.height);
                    let _ = pixels.resize_buffer(size.width, size.height);
            
                    let mut state = engine_state.borrow_mut();
                    state.frame.width = size.width;
                    state.frame.height = size.height;
                    state.frame.buffer.replace(vec![0; (size.width * size.height * 4) as usize]);
                }
            
                // Get mutable access to the buffer from state
                {
                    let state = engine_state.borrow_mut();
            
                    // Clear the buffer (optional — or up to app logic)
                    state.frame.buffer.borrow_mut().fill(0);
            
                    // Let the app draw into the buffer directly
                    app.tick(&*state);
                }
            
                // Copy final buffer into the pixels frame
                let frame = pixels.frame_mut();
                let state = engine_state.borrow();
                let buffer = state.frame.buffer.borrow();
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
                }

                WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                    size = *new_inner_size;
                    let _ = pixels.resize_surface(size.width, size.height);
                    let _ = pixels.resize_buffer(size.width, size.height);
                    window.request_redraw();
                }

                WindowEvent::CursorMoved { position, .. } => {
                    let mut state = engine_state.borrow_mut();
                    state.mouse.x = position.x as f32;
                    state.mouse.y = position.y as f32;
                    app.on_mouse_move(state.mouse.x, state.mouse.y);
                }

                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Left,
                    ..
                } => {
                    let mut state = engine_state.borrow_mut();
                    state.mouse.is_down = true;
                    app.on_mouse_down(state.mouse.x, state.mouse.y);
                }

                WindowEvent::MouseInput {
                    state: ElementState::Released,
                    button: MouseButton::Left,
                    ..
                } => {
                    let mut state = engine_state.borrow_mut();
                    state.mouse.is_down = false;
                    app.on_mouse_up(state.mouse.x, state.mouse.y);
                }

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

    let engine_state = Rc::new(RefCell::new(EngineState {
        frame: FrameState { width, height },
        mouse: MouseState { x: 0.0, y: 0.0, is_down: false },
    }));

    app.borrow_mut()
        .setup(&engine_state.borrow())
        .map_err(|e| JsValue::from_str(&e))?;

    // Mouse move
    {
        let state = engine_state.clone();
        let app_clone = app.clone();
        let move_callback = Closure::wrap(Box::new(move |event: MouseEvent| {
            let mut s = state.borrow_mut();
            s.mouse.x = event.offset_x() as f32;
            s.mouse.y = event.offset_y() as f32;
            app_clone.borrow_mut().on_mouse_move(s.mouse.x, s.mouse.y);
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("mousemove", move_callback.as_ref().unchecked_ref())?;
        move_callback.forget();
    }

    // Mouse down
    {
        let state = engine_state.clone();
        let app_clone = app.clone();
        let down_callback = Closure::wrap(Box::new(move |event: MouseEvent| {
            let mut s = state.borrow_mut();
            s.mouse.x = event.offset_x() as f32;
            s.mouse.y = event.offset_y() as f32;
            s.mouse.is_down = true;
            app_clone.borrow_mut().on_mouse_down(s.mouse.x, s.mouse.y);
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("mousedown", down_callback.as_ref().unchecked_ref())?;
        down_callback.forget();
    }

    // Mouse up
    {
        let state = engine_state.clone();
        let app_clone = app.clone();
        let up_callback = Closure::wrap(Box::new(move |event: MouseEvent| {
            let mut s = state.borrow_mut();
            s.mouse.x = event.offset_x() as f32;
            s.mouse.y = event.offset_y() as f32;
            s.mouse.is_down = false;
            app_clone.borrow_mut().on_mouse_up(s.mouse.x, s.mouse.y);
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("mouseup", up_callback.as_ref().unchecked_ref())?;
        up_callback.forget();
    }

    // Animation loop
    {
        let app_clone = app.clone();
        let state = engine_state.clone();
        let canvas_clone = canvas.clone();
        let context_clone = context.clone();

        let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
        let g = f.clone();

        *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
            let width = canvas_clone.width();
            let height = canvas_clone.height();

            {
                let mut s = state.borrow_mut();
                s.frame.width = width;
                s.frame.height = height;
            }

            let pixels = app_clone.borrow_mut().tick(&state.borrow());

            validate_frame_dimensions("wasm tick", width, height, &pixels);

            let data = wasm_bindgen::Clamped(&pixels[..]);
            let image_data = ImageData::new_with_u8_clamped_array_and_sh(data, width, height)
                .expect("Failed to create ImageData");
            context_clone
                .put_image_data(&image_data, 0.0, 0.0)
                .expect("put_image_data failed");

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
