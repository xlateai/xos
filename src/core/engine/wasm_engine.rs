#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

use super::{
    apply_frame_view_zoom,
    f3_menu_boost_interaction_fade,
    f3_menu_handle_frame_zoom_scroll,
    f3_menu_handle_mouse_down, f3_menu_handle_mouse_move, f3_menu_handle_mouse_up,
    f3_menu_handle_zoom_scroll, tick_f3_menu,
    frame_view_pan_by_pixels,
    tick_frame_view_zoom,
    F3Menu,
};
use super::engine::{
    tick_frame_delta, Application, CursorStyle, CursorStyleSetter, EngineState, FrameState,
    KeyboardState, MouseState, SafeRegionBoundingRectangle,
};


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
        command_held: bool,
        shift_held: bool,
        frame_pan_dragging: bool,
        paused_base_frame: Vec<u8>,
        paused_base_w: usize,
        paused_base_h: usize,
    }
    
    let state_ptr = Box::into_raw(Box::new(WasmState {
        engine_state: EngineState {
            frame: {
                let safe_region = SafeRegionBoundingRectangle::full_screen();
                FrameState::new(width, height, safe_region)
            },
            mouse: MouseState {
                x: 0.0,
                y: 0.0,
                dx: 0.0,
                dy: 0.0,
                is_left_clicking: false,
                is_right_clicking: false,
                style: CursorStyleSetter::new(),
            },
            keyboard: KeyboardState {
                onscreen: crate::ui::onscreen_keyboard::OnScreenKeyboard::new(),
            },
            f3_menu: F3Menu::new(),
            ui_scale_percent: 100,
            delta_time_seconds: 1.0 / 60.0,
            paused: false,
            pending_step_ticks: 0,
            frame_view_zoom: 1.0,
            frame_view_zoom_target: 1.0,
            frame_view_zoom_velocity: 0.0,
            frame_view_center_x: 0.5,
            frame_view_center_y: 0.5,
            f3_fps_label_override: None,
        },
        app,
        command_held: false,
        shift_held: false,
        frame_pan_dragging: false,
        paused_base_frame: Vec::new(),
        paused_base_w: 0,
        paused_base_h: 0,
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
        
                let new_x = event.offset_x() as f32;
                let new_y = event.offset_y() as f32;
        
                state.engine_state.mouse.dx = new_x - state.engine_state.mouse.x;
                state.engine_state.mouse.dy = new_y - state.engine_state.mouse.y;
        
                state.engine_state.mouse.x = new_x;
                state.engine_state.mouse.y = new_y;

                if state.frame_pan_dragging {
                    if !state.engine_state.mouse.is_left_clicking || !(state.command_held && state.shift_held) {
                        state.frame_pan_dragging = false;
                    }
                }

                if state.frame_pan_dragging {
                    f3_menu_boost_interaction_fade(&mut state.engine_state);
                    let shape = state.engine_state.frame.shape();
                    let pan_dx = state.engine_state.mouse.dx;
                    let pan_dy = state.engine_state.mouse.dy;
                    frame_view_pan_by_pixels(
                        &mut state.engine_state,
                        pan_dx,
                        pan_dy,
                        shape[1] as f32,
                        shape[0] as f32,
                    );
                    canvas_clone.style().set_property("cursor", "grabbing").unwrap();
                    return;
                }
        
                if !f3_menu_handle_mouse_move(&mut state.engine_state) {
                    state.app.on_mouse_move(&mut state.engine_state);
                }
        
                let cursor_style = state.engine_state.mouse.style.get();
                let style = match cursor_style {
                    CursorStyle::Default => "default",
                    CursorStyle::Text => "text",
                    CursorStyle::ResizeHorizontal => "ew-resize",
                    CursorStyle::ResizeVertical => "ns-resize",
                    CursorStyle::ResizeDiagonalNE => "nesw-resize",
                    CursorStyle::ResizeDiagonalNW => "nwse-resize",
                    CursorStyle::Hand => "pointer",
                    CursorStyle::Crosshair => "crosshair",
                    CursorStyle::Hidden => "none",
                };
                if state.command_held
                    && state.shift_held
                    && state.engine_state.frame_view_zoom > 1.001
                {
                    canvas_clone.style().set_property("cursor", "grab").unwrap();
                } else {
                    canvas_clone.style().set_property("cursor", style).unwrap();
                }
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
                if event.ctrl_key() || event.meta_key() {
                    let consumed = if event.shift_key() {
                        f3_menu_handle_frame_zoom_scroll(&mut state.engine_state, dy)
                    } else {
                        f3_menu_handle_zoom_scroll(&mut state.engine_state, dy)
                    };
                    if consumed {
                        event.prevent_default();
                        return;
                    }
                }
                if event.ctrl_key() || event.meta_key() {
                    event.prevent_default();
                }
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

                match event.button() {
                    0 => {
                        state.engine_state.mouse.is_left_clicking = true;
                        if state.command_held
                            && state.shift_held
                            && state.engine_state.frame_view_zoom > 1.001
                        {
                            state.frame_pan_dragging = true;
                            f3_menu_boost_interaction_fade(&mut state.engine_state);
                            event.prevent_default();
                            return;
                        }
                        if !f3_menu_handle_mouse_down(&mut state.engine_state) {
                            state.app.on_mouse_down(&mut state.engine_state);
                        }
                    }
                    2 => {
                        state.engine_state.mouse.is_right_clicking = true;
                        // Optionally call a separate handler here if needed
                    }
                    _ => {}
                }
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

                match event.button() {
                    0 => {
                        state.engine_state.mouse.is_left_clicking = false;
                        if state.frame_pan_dragging {
                            state.frame_pan_dragging = false;
                            event.prevent_default();
                            return;
                        }
                        if !f3_menu_handle_mouse_up(&mut state.engine_state) {
                            state.app.on_mouse_up(&mut state.engine_state);
                        }
                    }
                    2 => {
                        state.engine_state.mouse.is_right_clicking = false;
                        // Optionally call a separate handler here too
                    }
                    _ => {}
                }
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
                    let f3 = f3_menu_handle_mouse_move(&mut state.engine_state);
                    if !f3 {
                        state.app.on_mouse_move(&mut state.engine_state);
                    }

                    let dx = state.engine_state.mouse.x - prev_x;
                    let dy = state.engine_state.mouse.y - prev_y;
                    if state.engine_state.mouse.is_left_clicking && !f3 {
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
                    state.engine_state.mouse.is_left_clicking = true;
                    if !f3_menu_handle_mouse_down(&mut state.engine_state) {
                        state.app.on_mouse_down(&mut state.engine_state);
                    }
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
                state.engine_state.mouse.is_left_clicking = false;
                if !f3_menu_handle_mouse_up(&mut state.engine_state) {
                    state.app.on_mouse_up(&mut state.engine_state);
                }
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
                    "Control" | "Meta" => {
                        state.command_held = true;
                    }
                    "Shift" => {
                        state.shift_held = true;
                    }
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
                    "ArrowLeft" => {
                        state.app.on_key_char(&mut state.engine_state, '\u{2190}'); // ←
                        event.prevent_default();
                    }
                    "ArrowRight" => {
                        state.app.on_key_char(&mut state.engine_state, '\u{2192}'); // →
                        event.prevent_default();
                    }
                    "ArrowUp" => {
                        state.app.on_key_char(&mut state.engine_state, '\u{2191}'); // ↑
                        event.prevent_default();
                    }
                    "ArrowDown" => {
                        state.app.on_key_char(&mut state.engine_state, '\u{2193}'); // ↓
                        event.prevent_default();
                    }
                    "F3" => {
                        state.engine_state.f3_menu.toggle_visible();
                        event.prevent_default();
                    }
                    "Escape" | "Alt" | "CapsLock" | "Home" | "End" | "PageUp" | "PageDown" => {
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

    // Key up (modifier release for pan gesture)
    {
        use web_sys::KeyboardEvent;
        let state_ptr_clone = state_ptr;

        let keyup_callback = Closure::wrap(Box::new(move |event: KeyboardEvent| {
            unsafe {
                let state = &mut *state_ptr_clone;
                match event.key().as_str() {
                    "Control" | "Meta" => state.command_held = false,
                    "Shift" => state.shift_held = false,
                    _ => {}
                }
                if !(state.command_held && state.shift_held) {
                    state.frame_pan_dragging = false;
                }
            }
        }) as Box<dyn FnMut(_)>);

        window
            .add_event_listener_with_callback("keyup", keyup_callback.as_ref().unchecked_ref())?;
        keyup_callback.forget();
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
            last_tick_instant: Option<std::time::Instant>,
        }
        
        let anim_state_ptr = Box::into_raw(Box::new(AnimationState {
            callback: None,
            state_ptr,
            canvas: canvas.clone(),
            context: context.clone(),
            last_tick_instant: None,
        }));
        
        // Create the animation frame callback
        let callback = Closure::wrap(Box::new(move || {
            unsafe {
                let anim_state = &mut *anim_state_ptr;
                let state = &mut *anim_state.state_ptr;
                let width = anim_state.canvas.width();
                let height = anim_state.canvas.height();
                
                // Update dimensions if canvas size changed
                let shape = state.engine_state.frame.shape();
                if shape[1] as u32 != width || shape[0] as u32 != height {
                    state.engine_state.resize_frame(width, height);
                    // Notify app of screen size change
                    state.app.on_screen_size_change(&mut state.engine_state, width, height);
                }
                
                if state.engine_state.paused {
                    if state.engine_state.pending_step_ticks > 0 {
                        state.engine_state.pending_step_ticks = state.engine_state.pending_step_ticks.saturating_sub(1);
                        tick_frame_delta(
                            &mut state.engine_state,
                            &mut anim_state.last_tick_instant,
                        );
                        state.app.tick(&mut state.engine_state);
                        let shape = state.engine_state.frame.shape();
                        state.paused_base_w = shape[1];
                        state.paused_base_h = shape[0];
                        state.paused_base_frame = state.engine_state.frame.buffer_mut().to_vec();
                    } else {
                        anim_state.last_tick_instant = Some(std::time::Instant::now());
                        if state.paused_base_frame.is_empty() {
                            let shape = state.engine_state.frame.shape();
                            state.paused_base_w = shape[1];
                            state.paused_base_h = shape[0];
                            state.paused_base_frame = state.engine_state.frame.buffer_mut().to_vec();
                        }
                        if !state.paused_base_frame.is_empty() && state.paused_base_w > 0 && state.paused_base_h > 0 {
                            let shape = state.engine_state.frame.shape();
                            let dst_w = shape[1];
                            let dst_h = shape[0];
                            let dst = state.engine_state.frame.buffer_mut();
                            dst.fill(0);
                            let copy_w = state.paused_base_w.min(dst_w);
                            let copy_h = state.paused_base_h.min(dst_h);
                            let src_stride = state.paused_base_w * 4;
                            let dst_stride = dst_w * 4;
                            let row_bytes = copy_w * 4;
                            for y in 0..copy_h {
                                let src_off = y * src_stride;
                                let dst_off = y * dst_stride;
                                dst[dst_off..dst_off + row_bytes]
                                    .copy_from_slice(&state.paused_base_frame[src_off..src_off + row_bytes]);
                            }
                        }
                    }
                } else {
                    tick_frame_delta(
                        &mut state.engine_state,
                        &mut anim_state.last_tick_instant,
                    );
                    // Tick the app first
                    state.app.tick(&mut state.engine_state);
                    let shape = state.engine_state.frame.shape();
                    state.paused_base_w = shape[1];
                    state.paused_base_h = shape[0];
                    state.paused_base_frame = state.engine_state.frame.buffer_mut().to_vec();
                }

                tick_frame_view_zoom(&mut state.engine_state);
                apply_frame_view_zoom(&mut state.engine_state);
                
                // Then draw the keyboard on top (handles positioning, rendering, and key repeats)
                {
                    let mouse_x = state.engine_state.mouse.x;
                    let mouse_y = state.engine_state.mouse.y;
                    let safe_region = state.engine_state.frame.safe_region_boundaries.clone();
                    // Split borrows: get buffer and keyboard separately
                    let (buffer, keyboard) = {
                        let buffer_ptr = state.engine_state.frame.buffer_mut() as *mut [u8];
                        let keyboard_ptr: *mut crate::ui::onscreen_keyboard::OnScreenKeyboard = &mut state.engine_state.keyboard.onscreen;
                        (&mut *buffer_ptr, &mut *keyboard_ptr)
                    };
                    keyboard.tick(buffer, width, height, mouse_x, mouse_y, &safe_region);
                }

                tick_f3_menu(&mut state.engine_state);
                
                // Render to canvas
                let buffer = state.engine_state.frame_buffer_mut();
                let data = wasm_bindgen::Clamped(&buffer[..]);
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