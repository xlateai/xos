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
    pub buffer: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct MouseState {
    pub x: f32,
    pub y: f32,
    pub is_down: bool,
}

/// Trait that all XOS apps must implement
pub trait Application {
    fn setup(&mut self, state: &mut EngineState) -> Result<(), String>;
    fn tick(&mut self, state: &mut EngineState);
    
    fn on_mouse_down(&mut self, _state: &mut EngineState) {}
    fn on_mouse_up(&mut self, _state: &mut EngineState) {}
    fn on_mouse_move(&mut self, _state: &mut EngineState) {}
}

/// Shared function to validate frame buffer size
// fn validate_frame_dimensions(label: &str, width: u32, height: u32, buffer: &[u8]) {
//     let expected = (width * height * 4) as usize;
//     let actual = buffer.len();
//     if expected != actual {
//         #[cfg(target_arch = "wasm32")]
//         web_sys::console::error_1(&format!(
//             "[{label}] Frame size mismatch: expected {} ({}x{}x4), got {}",
//             expected, width, height, actual
//         ).into());

//         #[cfg(not(target_arch = "wasm32"))]
//         eprintln!(
//             "[{label}] Frame size mismatch: expected {} ({}x{}x4), got {}",
//             expected, width, height, actual
//         );
//     }
// }

#[cfg(not(target_arch = "wasm32"))]
pub fn start_native(mut app: Box<dyn Application>) -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("XOS Game")
        .build(&event_loop)?;

    let mut size = window.inner_size();
    let surface_texture = SurfaceTexture::new(size.width, size.height, &window);
    let mut pixels = Pixels::new(size.width, size.height, surface_texture)?;

    let mut engine_state = EngineState {
        frame: FrameState {
            width: size.width,
            height: size.height,
            buffer: vec![0; (size.width * size.height * 4) as usize],
        },
        mouse: MouseState {
            x: 0.0,
            y: 0.0,
            is_down: false,
        },
    };

    app.setup(&mut engine_state)?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        match event {
            Event::RedrawRequested(_) => {
                let current_size = window.inner_size();
            
                if current_size != size {
                    // Update the window size first
                    size = current_size;
                    
                    // Resize the pixels buffer first
                    let _ = pixels.resize_buffer(size.width, size.height);
                    let _ = pixels.resize_surface(size.width, size.height);
                    
                    // Then update our engine state to match
                    engine_state.frame.width = size.width;
                    engine_state.frame.height = size.height;
                    engine_state.frame.buffer = vec![0; (size.width * size.height * 4) as usize];
                }
                
                // Update the game state
                engine_state.frame.buffer.fill(0);
                app.tick(&mut engine_state);
            
                // Ensure sizes match before copying
                let frame = pixels.frame_mut();
                if frame.len() == engine_state.frame.buffer.len() {
                    frame.copy_from_slice(&engine_state.frame.buffer);
                    let _ = pixels.render();
                } else {
                    // Resize buffer if mismatch detected
                    engine_state.frame.buffer.resize(frame.len(), 0);
                    eprintln!("Buffer size mismatch detected and fixed. New size: {}", frame.len());
                }
            }

            // Rest of the code remains the same...
            Event::MainEventsCleared => {
                window.request_redraw();
            }

            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,

                WindowEvent::Resized(new_size) => {
                    size = new_size;
                    // Update pixels first
                    let _ = pixels.resize_buffer(size.width, size.height);
                    let _ = pixels.resize_surface(size.width, size.height);
                    
                    // Then update engine state
                    engine_state.frame.width = size.width;
                    engine_state.frame.height = size.height;
                    engine_state.frame.buffer = vec![0; (size.width * size.height * 4) as usize];
                    
                    window.request_redraw();
                }

                WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                    size = *new_inner_size;
                    // Update pixels first
                    let _ = pixels.resize_buffer(size.width, size.height);
                    let _ = pixels.resize_surface(size.width, size.height);
                    
                    // Then update engine state
                    engine_state.frame.width = size.width;
                    engine_state.frame.height = size.height;
                    engine_state.frame.buffer = vec![0; (size.width * size.height * 4) as usize];
                    
                    window.request_redraw();
                }

                WindowEvent::CursorMoved { position, .. } => {
                    engine_state.mouse.x = position.x as f32;
                    engine_state.mouse.y = position.y as f32;
                    app.on_mouse_move(&mut engine_state);
                }

                WindowEvent::MouseInput {
                    state: button_state,
                    button: MouseButton::Left,
                    ..
                } => {
                    match button_state {
                        ElementState::Pressed => {
                            engine_state.mouse.is_down = true;
                            app.on_mouse_down(&mut engine_state);
                        }
                        ElementState::Released => {
                            engine_state.mouse.is_down = false;
                            app.on_mouse_up(&mut engine_state);
                        }
                    }
                }

                _ => {}
            },

            _ => {}
        }
    });
}



#[cfg(target_arch = "wasm32")]
pub fn run_web(mut app: Box<dyn Application>) -> Result<(), JsValue> {
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

    // Create a struct to store wasm state and share safely
    struct WasmState {
        engine_state: EngineState,
        app: Box<dyn Application>,
    }
    
    let state_ptr = Box::into_raw(Box::new(WasmState {
        engine_state: EngineState {
            frame: FrameState {
                width,
                height,
                buffer: vec![0; (width * height * 4) as usize],
            },
            mouse: MouseState {
                x: 0.0,
                y: 0.0,
                is_down: false,
            },
        },
        app,
    }));
    
    // Setup the app
    unsafe {
        (*state_ptr).app.setup(&mut (*state_ptr).engine_state)
            .map_err(|e| JsValue::from_str(&e))?;
    }

    // Mouse move
    {
        let state_ptr_clone = state_ptr;
        let move_callback = Closure::wrap(Box::new(move |event: MouseEvent| {
            unsafe {
                let state = &mut *state_ptr_clone;
                state.engine_state.mouse.x = event.offset_x() as f32;
                state.engine_state.mouse.y = event.offset_y() as f32;
                state.app.on_mouse_move(&mut state.engine_state);
            }
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("mousemove", move_callback.as_ref().unchecked_ref())?;
        move_callback.forget();
    }

    // Mouse down
    {
        let state_ptr_clone = state_ptr;
        let down_callback = Closure::wrap(Box::new(move |event: MouseEvent| {
            unsafe {
                let state = &mut *state_ptr_clone;
                state.engine_state.mouse.x = event.offset_x() as f32;
                state.engine_state.mouse.y = event.offset_y() as f32;
                state.engine_state.mouse.is_down = true;
                state.app.on_mouse_down(&mut state.engine_state);
            }
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("mousedown", down_callback.as_ref().unchecked_ref())?;
        down_callback.forget();
    }

    // Mouse up
    {
        let state_ptr_clone = state_ptr;
        let up_callback = Closure::wrap(Box::new(move |event: MouseEvent| {
            unsafe {
                let state = &mut *state_ptr_clone;
                state.engine_state.mouse.x = event.offset_x() as f32;
                state.engine_state.mouse.y = event.offset_y() as f32;
                state.engine_state.mouse.is_down = false;
                state.app.on_mouse_up(&mut state.engine_state);
            }
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("mouseup", up_callback.as_ref().unchecked_ref())?;
        up_callback.forget();
    }

    // Animation loop - use a different approach without Rc/RefCell
    {
        // Store the callback in a static location
        // Use *mut for direct pointer access instead of Rc/RefCell
        struct AnimationState {
            callback: Option<Closure<dyn FnMut()>>,
            state_ptr: *mut WasmState,
            canvas: HtmlCanvasElement,
            context: CanvasRenderingContext2d,
        }
        
        let anim_state_ptr = Box::into_raw(Box::new(AnimationState {
            callback: None,
            state_ptr,
            canvas: canvas.clone(),
            context: context.clone(),
        }));
        
        // Create the animation frame callback
        let callback = Closure::wrap(Box::new(move || {
            unsafe {
                let anim_state = &mut *anim_state_ptr;
                let state = &mut *anim_state.state_ptr;
                let width = anim_state.canvas.width();
                let height = anim_state.canvas.height();
                
                // Update dimensions if canvas size changed
                if state.engine_state.frame.width != width || state.engine_state.frame.height != height {
                    state.engine_state.frame.width = width;
                    state.engine_state.frame.height = height;
                    state.engine_state.frame.buffer = vec![0; (width * height * 4) as usize];
                }
                
                // Update game state
                state.engine_state.frame.buffer.fill(0);
                state.app.tick(&mut state.engine_state);
                
                // Render to canvas
                // validate_frame_dimensions(
                //     "wasm tick", 
                //     width, 
                //     height, 
                //     &state.engine_state.frame.buffer
                // );
                
                let data = wasm_bindgen::Clamped(&state.engine_state.frame.buffer[..]);
                let image_data = ImageData::new_with_u8_clamped_array_and_sh(data, width, height)
                    .expect("Failed to create ImageData");
                    
                anim_state.context
                    .put_image_data(&image_data, 0.0, 0.0)
                    .expect("put_image_data failed");
                
                // Request next animation frame
                web_sys::window()
                    .unwrap()
                    .request_animation_frame(anim_state.callback.as_ref().unwrap().as_ref().unchecked_ref())
                    .expect("requestAnimationFrame failed");
            }
        }) as Box<dyn FnMut()>);
        
        // Store the callback in our state
        unsafe {
            (*anim_state_ptr).callback = Some(callback);
            
            // Start the animation loop
            web_sys::window()
                .unwrap()
                .request_animation_frame((*anim_state_ptr).callback.as_ref().unwrap().as_ref().unchecked_ref())
                .expect("Initial requestAnimationFrame failed");
        }
        
        // Intentionally leak the animation state - it will live for the lifetime of the application
        // (this is typical for WASM web applications that don't have a clear shutdown path)
    }

    Ok(())
}