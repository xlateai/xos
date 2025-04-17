#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

use super::engine::{Application, EngineState, FrameState, MouseState, CursorStyle, CursorStyleSetter};


#[cfg(target_arch = "wasm32")]
pub fn run_web(app: Box<dyn Application>) -> Result<(), JsValue> {
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
                style: CursorStyleSetter::new(),
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
        let canvas_clone = canvas.clone();
        let move_callback = Closure::wrap(Box::new(move |event: MouseEvent| {
            unsafe {
                let state = &mut *state_ptr_clone;
                state.engine_state.mouse.x = event.offset_x() as f32;
                state.engine_state.mouse.y = event.offset_y() as f32;
                state.app.on_mouse_move(&mut state.engine_state);

                let style = match state.engine_state.mouse.style.get() {
                    CursorStyle::Default => "default",
                    CursorStyle::Text => "text",
                    CursorStyle::ResizeHorizontal => "ew-resize",
                    CursorStyle::ResizeVertical => "ns-resize",
                    CursorStyle::ResizeDiagonalNE => "nesw-resize",
                    CursorStyle::ResizeDiagonalNW => "nwse-resize",
                    CursorStyle::Hand => "pointer",
                    CursorStyle::Crosshair => "crosshair",
                };

                canvas_clone.style().set_property("cursor", style).unwrap();
            }
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("mousemove", move_callback.as_ref().unchecked_ref())?;
        move_callback.forget();
    }

    // Mouse Scroll
    {
        use web_sys::WheelEvent;
    
        let state_ptr_clone = state_ptr;
        let scroll_callback = Closure::wrap(Box::new(move |event: WheelEvent| {
            unsafe {
                let state = &mut *state_ptr_clone;
                let dx = event.delta_x() as f32;
                let dy = -event.delta_y() as f32;
                state.app.on_scroll(&mut state.engine_state, dx, dy);
            }
        }) as Box<dyn FnMut(_)>);
    
        canvas.add_event_listener_with_callback("wheel", scroll_callback.as_ref().unchecked_ref())?;
        scroll_callback.forget();
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

    // Touch move (acts like mouse move + drag-to-scroll)
    {
        use web_sys::TouchEvent;
        let state_ptr_clone = state_ptr;
        let canvas_clone = canvas.clone(); // ✅ clone canvas here

        let touch_move_callback = Closure::wrap(Box::new(move |event: TouchEvent| {
            unsafe {
                let state = &mut *state_ptr_clone;
                if let Some(touch) = event.touches().get(0) {
                    let rect = canvas_clone.get_bounding_client_rect(); // ✅ use cloned version
                    let x = touch.client_x() as f64 - rect.left();
                    let y = touch.client_y() as f64 - rect.top();
                    let prev_x = state.engine_state.mouse.x;
                    let prev_y = state.engine_state.mouse.y;
                    state.engine_state.mouse.x = x as f32;
                    state.engine_state.mouse.y = y as f32;
                    state.app.on_mouse_move(&mut state.engine_state);

                    let dx = state.engine_state.mouse.x - prev_x;
                    let dy = state.engine_state.mouse.y - prev_y;
                    if state.engine_state.mouse.is_down {
                        state.app.on_scroll(&mut state.engine_state, -dx, -dy);
                    }
                }
                event.prevent_default();
            }
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("touchmove", touch_move_callback.as_ref().unchecked_ref())?;
        touch_move_callback.forget();
    }

    // Touch start
    {
        use web_sys::TouchEvent;
        let state_ptr_clone = state_ptr;
        let canvas_clone = canvas.clone(); // ✅ clone canvas here

        let touch_start_callback = Closure::wrap(Box::new(move |event: TouchEvent| {
            unsafe {
                let state = &mut *state_ptr_clone;
                if let Some(touch) = event.touches().get(0) {
                    let rect = canvas_clone.get_bounding_client_rect(); // ✅ use cloned version
                    let x = touch.client_x() as f64 - rect.left();
                    let y = touch.client_y() as f64 - rect.top();
                    state.engine_state.mouse.x = x as f32;
                    state.engine_state.mouse.y = y as f32;
                    state.engine_state.mouse.is_down = true;
                    state.app.on_mouse_down(&mut state.engine_state);
                }
                event.prevent_default();
            }
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("touchstart", touch_start_callback.as_ref().unchecked_ref())?;
        touch_start_callback.forget();
    }

    // Touch end
    {
        use web_sys::TouchEvent;
        let state_ptr_clone = state_ptr;

        let touch_end_callback = Closure::wrap(Box::new(move |event: TouchEvent| {
            unsafe {
                let state = &mut *state_ptr_clone;
                state.engine_state.mouse.is_down = false;
                state.app.on_mouse_up(&mut state.engine_state);
                event.prevent_default();
            }
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("touchend", touch_end_callback.as_ref().unchecked_ref())?;
        touch_end_callback.forget();
    }

    // Keyboard input
    {
        use web_sys::KeyboardEvent;
        let state_ptr_clone = state_ptr;

        let keydown_callback = Closure::wrap(Box::new(move |event: KeyboardEvent| {
            unsafe {
                let state = &mut *state_ptr_clone;
                let key = event.key();
        
                match key.as_str() {
                    "Enter" => {
                        state.app.on_key_char(&mut state.engine_state, '\n');
                        event.prevent_default();
                    }
                    "Backspace" => {
                        state.app.on_key_char(&mut state.engine_state, '\u{8}');
                        event.prevent_default();
                    }
                    "Tab" => {
                        state.app.on_key_char(&mut state.engine_state, '\t');
                        event.prevent_default();
                    }
                    "Escape" | "Shift" | "Control" | "Alt" | "Meta" | "CapsLock" | "ArrowLeft" | "ArrowRight"
                    | "ArrowUp" | "ArrowDown" | "Home" | "End" | "PageUp" | "PageDown" => {
                        // Do nothing — non-character keys
                    }
                    _ => {
                        // If it's a single printable char, send it
                        if key.len() == 1 && !event.ctrl_key() && !event.meta_key() && !event.alt_key() {
                            if let Some(c) = key.chars().next() {
                                state.app.on_key_char(&mut state.engine_state, c);
                            }
                        }
                    }
                }
            }
        }) as Box<dyn FnMut(_)>);

        window
            .add_event_listener_with_callback("keydown", keydown_callback.as_ref().unchecked_ref())?;
        keydown_callback.forget();
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