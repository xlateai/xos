use rustpython_vm::{PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs};
use std::sync::Mutex;
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
use std::time::Duration;

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
use pixels::{Pixels, PixelsBuilder, SurfaceTexture};
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, NamedKey},
    platform::pump_events::{EventLoopExtPumpEvents, PumpStatus},
    window::{Window, WindowAttributes, WindowId},
};
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
use std::cell::RefCell;

static STANDALONE_FRAME_BUFFER: Mutex<Vec<u8>> = Mutex::new(Vec::new());

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
struct StandalonePreviewState {
    window: Window,
    pixels: Pixels<'static>,
    size: PhysicalSize<u32>,
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
struct StandalonePreviewApp {
    state: Option<StandalonePreviewState>,
    width: u32,
    height: u32,
    frame_rgba: Vec<u8>,
    should_close: bool,
    f3_engine_state: Option<crate::engine::EngineState>,
    last_tick_instant: Option<std::time::Instant>,
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
impl StandalonePreviewApp {
    fn new() -> Self {
        Self {
            state: None,
            width: 800,
            height: 600,
            frame_rgba: Vec::new(),
            should_close: false,
            f3_engine_state: None,
            last_tick_instant: None,
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
impl ApplicationHandler for StandalonePreviewApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let attrs = WindowAttributes::default()
            .with_title("xos standalone preview")
            .with_inner_size(PhysicalSize::new(self.width, self.height));
        let window = match event_loop.create_window(attrs) {
            Ok(w) => w,
            Err(_) => return,
        };
        let size = window.inner_size();
        let surface_texture = SurfaceTexture::new(size.width, size.height, &window);
        let pixels = match PixelsBuilder::new(size.width, size.height, surface_texture)
            .enable_vsync(false)
            .build()
        {
            Ok(p) => unsafe { std::mem::transmute(p) },
            Err(_) => return,
        };
        let safe_region = crate::engine::SafeRegionBoundingRectangle::full_screen();
        let f3_engine_state = crate::engine::EngineState {
            frame: crate::engine::FrameState::new(size.width.max(1), size.height.max(1), safe_region),
            mouse: crate::engine::MouseState {
                x: 0.0,
                y: 0.0,
                dx: 0.0,
                dy: 0.0,
                is_left_clicking: false,
                is_right_clicking: false,
                style: crate::engine::CursorStyleSetter::new(),
            },
            keyboard: crate::engine::KeyboardState {
                onscreen: crate::ui::onscreen_keyboard::OnScreenKeyboard::new(),
            },
            f3_menu: crate::engine::F3Menu::new(),
            ui_scale_percent: 50,
            delta_time_seconds: 1.0 / 60.0,
        };
        self.f3_engine_state = Some(f3_engine_state);
        self.state = Some(StandalonePreviewState { window, pixels, size });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        let Some(state) = self.state.as_mut() else { return; };
        match event {
            WindowEvent::CloseRequested => {
                self.should_close = true;
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                if new_size.width > 0 && new_size.height > 0 {
                    state.size = new_size;
                    let _ = state.pixels.resize_buffer(new_size.width, new_size.height);
                    let _ = state.pixels.resize_surface(new_size.width, new_size.height);
                    if let Some(es) = self.f3_engine_state.as_mut() {
                        es.resize_frame(new_size.width, new_size.height);
                        let _ = crate::engine::f3_menu_handle_mouse_move(es);
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(es) = self.f3_engine_state.as_mut() {
                    es.mouse.dx = position.x as f32 - es.mouse.x;
                    es.mouse.dy = position.y as f32 - es.mouse.y;
                    es.mouse.x = position.x as f32;
                    es.mouse.y = position.y as f32;
                    let _ = crate::engine::f3_menu_handle_mouse_move(es);
                }
            }
            WindowEvent::MouseInput {
                state: button_state,
                button: MouseButton::Left,
                ..
            } => {
                if let Some(es) = self.f3_engine_state.as_mut() {
                    match button_state {
                        ElementState::Pressed => {
                            es.mouse.is_left_clicking = true;
                            let _ = crate::engine::f3_menu_handle_mouse_down(es);
                        }
                        ElementState::Released => {
                            es.mouse.is_left_clicking = false;
                            let _ = crate::engine::f3_menu_handle_mouse_up(es);
                        }
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed
                    && matches!(event.logical_key, Key::Named(NamedKey::F3))
                {
                    if let Some(es) = self.f3_engine_state.as_mut() {
                        es.f3_menu.toggle_visible();
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let expected = (state.size.width as usize)
                    .saturating_mul(state.size.height as usize)
                    .saturating_mul(4);
                if self.frame_rgba.len() == expected {
                    if let Some(es) = self.f3_engine_state.as_mut() {
                        let frame = es.frame.buffer_mut();
                        if frame.len() == self.frame_rgba.len() {
                            frame.copy_from_slice(&self.frame_rgba);
                            crate::engine::tick_frame_delta(es, &mut self.last_tick_instant);
                            crate::engine::tick_f3_menu(es);
                            self.frame_rgba.copy_from_slice(es.frame.buffer_mut());
                        }
                    }
                    state.pixels.frame_mut().copy_from_slice(&self.frame_rgba);
                    let _ = state.pixels.render();
                } else {
                    let _ = state.pixels.render();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.should_close {
            event_loop.exit();
            return;
        }
        if let Some(state) = self.state.as_ref() {
            state.window.request_redraw();
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
struct StandalonePreviewHost {
    event_loop: EventLoop<()>,
    app: StandalonePreviewApp,
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
thread_local! {
    static STANDALONE_PREVIEW_HOST: RefCell<Option<StandalonePreviewHost>> = const { RefCell::new(None) };
}

/// The xos.hello() function
fn hello(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    println!("hello from xos module");
    Ok(vm.ctx.none())
}

/// xos.get_frame_buffer() - returns the frame buffer dimensions and a placeholder
/// In the future, this will return actual frame buffer access
fn get_frame_buffer(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    // For now, return a dict with width, height, and placeholder buffer
    let dict = vm.ctx.new_dict();
    dict.set_item("width", vm.ctx.new_int(800).into(), vm)?;
    dict.set_item("height", vm.ctx.new_int(600).into(), vm)?;
    dict.set_item("buffer", vm.ctx.new_list(vec![]).into(), vm)?;
    Ok(dict.into())
}

/// xos.get_mouse() - returns mouse position
fn get_mouse(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let dict = vm.ctx.new_dict();
    dict.set_item("x", vm.ctx.new_float(0.0).into(), vm)?;
    dict.set_item("y", vm.ctx.new_float(0.0).into(), vm)?;
    dict.set_item("down", vm.ctx.new_bool(false).into(), vm)?;
    Ok(dict.into())
}

/// xos.print() - alias to builtin print (no longer needed, kept for compatibility)
/// We'll set this to builtins.print in make_module instead

/// xos.sleep() - sleep for a number of seconds
/// NOTE: This blocks the main thread, so it's not recommended for use in the coder app
/// For periodic updates, use a viewport app with tick() instead
fn xos_sleep(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let seconds: f64 = args.bind(vm)?;
    let duration = std::time::Duration::from_secs_f64(seconds);
    std::thread::sleep(duration);
    Ok(vm.ctx.none())
}

/// xos.frame.clear(...) - clear current frame buffer context
/// Supports:
/// - clear()
/// - clear((r, g, b)) or clear((r, g, b, a))
/// - clear(r, g, b) or clear(r, g, b, a)
fn frame_clear(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;

    let parse_rgba_from_tuple = |tuple_obj: &rustpython_vm::builtins::PyTuple| -> PyResult<(i32, i32, i32, i32)> {
        let items = tuple_obj.as_slice();
        if items.len() == 3 {
            let r: i32 = items[0].clone().try_into_value(vm)?;
            let g: i32 = items[1].clone().try_into_value(vm)?;
            let b: i32 = items[2].clone().try_into_value(vm)?;
            Ok((r, g, b, 255))
        } else if items.len() == 4 {
            let r: i32 = items[0].clone().try_into_value(vm)?;
            let g: i32 = items[1].clone().try_into_value(vm)?;
            let b: i32 = items[2].clone().try_into_value(vm)?;
            let a: i32 = items[3].clone().try_into_value(vm)?;
            Ok((r, g, b, a))
        } else {
            Err(vm.new_type_error("color tuple must be (r, g, b) or (r, g, b, a)".to_string()))
        }
    };

    let (r, g, b, a): (i32, i32, i32, i32) = match args_vec.len() {
        0 => (0, 0, 0, 255),
        1 => {
            let color_tuple = args_vec[0]
                .downcast_ref::<rustpython_vm::builtins::PyTuple>()
                .ok_or_else(|| vm.new_type_error("clear(color): color must be a tuple".to_string()))?;
            parse_rgba_from_tuple(color_tuple)?
        }
        3 => {
            let r: i32 = args_vec[0].clone().try_into_value(vm)?;
            let g: i32 = args_vec[1].clone().try_into_value(vm)?;
            let b: i32 = args_vec[2].clone().try_into_value(vm)?;
            (r, g, b, 255)
        }
        4 => {
            let r: i32 = args_vec[0].clone().try_into_value(vm)?;
            let g: i32 = args_vec[1].clone().try_into_value(vm)?;
            let b: i32 = args_vec[2].clone().try_into_value(vm)?;
            let a: i32 = args_vec[3].clone().try_into_value(vm)?;
            (r, g, b, a)
        }
        _ => {
            return Err(vm.new_type_error(
                "clear() accepts (), (r,g,b), (r,g,b,a), r,g,b, or r,g,b,a".to_string(),
            ));
        }
    };

    let buffer_ptr_opt = crate::python_api::rasterizer::CURRENT_FRAME_BUFFER
        .lock()
        .unwrap()
        .as_ref()
        .map(|ptr| ptr.0);
    let width = *crate::python_api::rasterizer::CURRENT_FRAME_WIDTH.lock().unwrap();
    let height = *crate::python_api::rasterizer::CURRENT_FRAME_HEIGHT.lock().unwrap();

    let buffer_ptr = buffer_ptr_opt.ok_or_else(|| {
        vm.new_runtime_error("No frame buffer context set. frame.clear must be called during tick().".to_string())
    })?;

    let buffer_len = width * height * 4;
    let buffer = unsafe { std::slice::from_raw_parts_mut(buffer_ptr, buffer_len) };
    crate::python_api::rasterizer::fill_buffer_solid_rgba(
        buffer,
        r.clamp(0, 255) as u8,
        g.clamp(0, 255) as u8,
        b.clamp(0, 255) as u8,
        a.clamp(0, 255) as u8,
    );

    Ok(vm.ctx.none())
}

/// xos.frame._begin_standalone(width=800, height=600) -> frame dict
/// Initializes a temporary framebuffer context so `app.tick()` can be called directly from Python.
fn frame_begin_standalone(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    let width: usize = if !args_vec.is_empty() {
        let w: i32 = args_vec[0].clone().try_into_value(vm)?;
        w.max(1) as usize
    } else {
        800
    };
    let height: usize = if args_vec.len() > 1 {
        let h: i32 = args_vec[1].clone().try_into_value(vm)?;
        h.max(1) as usize
    } else {
        600
    };

    {
        let mut buf = STANDALONE_FRAME_BUFFER
            .lock()
            .map_err(|_| vm.new_runtime_error("standalone frame buffer lock poisoned".to_string()))?;
        let required = width.saturating_mul(height).saturating_mul(4);
        if buf.len() != required {
            buf.resize(required, 0);
        }
        crate::python_api::rasterizer::set_frame_buffer_context(buf.as_mut_slice(), width, height);
    }

    let tensor_dict = vm.ctx.new_dict();
    tensor_dict.set_item(
        "shape",
        vm.ctx
            .new_tuple(vec![
                vm.ctx.new_int(height).into(),
                vm.ctx.new_int(width).into(),
                vm.ctx.new_int(4).into(),
            ])
            .into(),
        vm,
    )?;
    tensor_dict.set_item("device", vm.ctx.new_str("cpu").into(), vm)?;
    tensor_dict.set_item("dtype", vm.ctx.new_str("uint8").into(), vm)?;
    tensor_dict.set_item("size", vm.ctx.new_int(width * height * 4).into(), vm)?;

    let frame_dict = vm.ctx.new_dict();
    frame_dict.set_item("width", vm.ctx.new_int(width).into(), vm)?;
    frame_dict.set_item("height", vm.ctx.new_int(height).into(), vm)?;
    frame_dict.set_item("tensor", tensor_dict.into(), vm)?;
    Ok(frame_dict.into())
}

/// xos.frame._end_standalone() - clears temporary standalone framebuffer context.
fn frame_end_standalone(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    crate::python_api::rasterizer::clear_frame_buffer_context();
    Ok(vm.ctx.none())
}

/// xos.frame._standalone_window_size() -> (width, height) | None
/// Returns the current standalone preview window size when available.
fn frame_standalone_window_size(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    #[cfg(any(target_arch = "wasm32", target_os = "ios"))]
    {
        Ok(vm.ctx.none())
    }
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    {
        let maybe_size = STANDALONE_PREVIEW_HOST.with(|slot| {
            slot.borrow()
                .as_ref()
                .and_then(|host| host.app.state.as_ref().map(|s| (s.size.width, s.size.height)))
        });
        if let Some((w, h)) = maybe_size {
            Ok(vm
                .ctx
                .new_tuple(vec![vm.ctx.new_int(w as usize).into(), vm.ctx.new_int(h as usize).into()])
                .into())
        } else {
            Ok(vm.ctx.none())
        }
    }
}

/// xos.frame._present_standalone() - presents standalone buffer in a non-blocking native window.
fn frame_present_standalone(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    #[cfg(any(target_arch = "wasm32", target_os = "ios"))]
    {
        return Ok(vm.ctx.none());
    }
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    {
        let width = *crate::python_api::rasterizer::CURRENT_FRAME_WIDTH.lock().unwrap() as u32;
        let height = *crate::python_api::rasterizer::CURRENT_FRAME_HEIGHT.lock().unwrap() as u32;
        let frame = STANDALONE_FRAME_BUFFER
            .lock()
            .map_err(|_| vm.new_runtime_error("standalone frame buffer lock poisoned".to_string()))?
            .clone();

        STANDALONE_PREVIEW_HOST.with(|slot| {
            let mut opt = slot.borrow_mut();
            if opt.is_none() {
                let event_loop = EventLoop::new()
                    .map_err(|e| vm.new_runtime_error(format!("failed to create preview event loop: {e}")))?;
                *opt = Some(StandalonePreviewHost {
                    event_loop,
                    app: StandalonePreviewApp::new(),
                });
            }
            if let Some(host) = opt.as_mut() {
                host.app.width = width.max(1);
                host.app.height = height.max(1);
                host.app.frame_rgba = frame;
                match host
                    .event_loop
                    .pump_app_events(Some(Duration::ZERO), &mut host.app)
                {
                    PumpStatus::Continue => {}
                    PumpStatus::Exit(_) => {
                        *opt = None;
                    }
                }
            }
            Ok(())
        })?;
        Ok(vm.ctx.none())
    }
}

/// Create the xos module with Application base class
pub fn make_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos", vm.ctx.new_dict(), None);
    
    // Add functions to the module
    module.set_attr("hello", vm.new_function("hello", hello), vm).unwrap();
    module.set_attr("get_frame_buffer", vm.new_function("get_frame_buffer", get_frame_buffer), vm).unwrap();
    module.set_attr("get_mouse", vm.new_function("get_mouse", get_mouse), vm).unwrap();
    
    // Make xos.print an alias to the builtin print function
    if let Ok(builtin_print) = vm.builtins.get_attr("print", vm) {
        module.set_attr("print", builtin_print, vm).unwrap();
    }
    
    module.set_attr("sleep", vm.new_function("sleep", xos_sleep), vm).unwrap();
    
    // Add the random submodule
    let random_module = crate::python_api::random::random::make_random_module(vm);
    module.set_attr("random", random_module, vm).unwrap();

    // Add a lightweight string constants submodule for Python compatibility.
    let string_module = vm.new_module("xos.string", vm.ctx.new_dict(), None);
    string_module
        .set_attr(
            "ascii_letters",
            vm.ctx.new_str("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"),
            vm,
        )
        .unwrap();
    string_module
        .set_attr("digits", vm.ctx.new_str("0123456789"), vm)
        .unwrap();
    module.set_attr("string", string_module, vm).unwrap();
    
    // Add the rasterizer submodule
    let rasterizer_module = crate::python_api::rasterizer::make_rasterizer_module(vm);
    module.set_attr("rasterizer", rasterizer_module, vm).unwrap();

    // Add the frame submodule
    let frame_module = vm.new_module("xos.frame", vm.ctx.new_dict(), None);
    frame_module
        .set_attr("clear", vm.new_function("clear", frame_clear), vm)
        .unwrap();
    frame_module
        .set_attr("_begin_standalone", vm.new_function("_begin_standalone", frame_begin_standalone), vm)
        .unwrap();
    frame_module
        .set_attr("_end_standalone", vm.new_function("_end_standalone", frame_end_standalone), vm)
        .unwrap();
    frame_module
        .set_attr(
            "_standalone_window_size",
            vm.new_function("_standalone_window_size", frame_standalone_window_size),
            vm,
        )
        .unwrap();
    frame_module
        .set_attr("_present_standalone", vm.new_function("_present_standalone", frame_present_standalone), vm)
        .unwrap();
    module.set_attr("frame", frame_module, vm).unwrap();

    // Add color palette submodule (single source of truth in py/colors.rs)
    let color_module = crate::python_api::colors::make_color_module(vm);
    module.set_attr("color", color_module, vm).unwrap();
    
    // Add the sensors submodule
    let sensors_module = crate::python_api::sensors::make_sensors_module(vm);
    module.set_attr("sensors", sensors_module, vm).unwrap();
    
    // Add the audio submodule
    let audio_module = crate::python_api::audio::make_audio_module(vm);
    module.set_attr("audio", audio_module, vm).unwrap();
    
    // Add the system submodule
    let system_module = crate::python_api::system::make_system_module(vm);
    module.set_attr("system", system_module, vm).unwrap();
    
    // Add the dialoguer submodule
    let dialoguer_module = crate::python_api::dialoguer::make_dialoguer_module(vm);
    module.set_attr("dialoguer", dialoguer_module, vm).unwrap();
    
    // Add the math submodule
    let math_module = crate::python_api::math::make_math_module(vm);
    module.set_attr("math", math_module, vm).unwrap();
    
    // Add the ops submodule
    let ops_module = crate::python_api::ops::make_ops_module(vm);
    module.set_attr("ops", ops_module, vm).unwrap();
    
    // Add the tensors submodule (Burn-backed, replaces array)
    let tensors_module = crate::python_api::tensors::make_tensors_module(vm);
    module.set_attr("tensor", tensors_module.get_attr("tensor", vm).unwrap(), vm).unwrap();
    module.set_attr("zeros", tensors_module.get_attr("zeros", vm).unwrap(), vm).unwrap();
    module.set_attr("ones", tensors_module.get_attr("ones", vm).unwrap(), vm).unwrap();
    module.set_attr("full", tensors_module.get_attr("full", vm).unwrap(), vm).unwrap();
    module.set_attr("arange", tensors_module.get_attr("arange", vm).unwrap(), vm).unwrap();
    module.set_attr("stack", tensors_module.get_attr("stack", vm).unwrap(), vm).unwrap();
    module.set_attr("where", tensors_module.get_attr("where", vm).unwrap(), vm).unwrap();
    module.set_attr("clip", tensors_module.get_attr("clip", vm).unwrap(), vm).unwrap();
    
    // Add the data submodule
    let data_module = crate::python_api::data::make_data_module(vm);
    module.set_attr("data", data_module, vm).unwrap();
    
    // Add the ui submodule
    let ui_module = crate::python_api::ui::make_ui_module(vm);
    module.set_attr("ui", ui_module, vm).unwrap();
    
    // Add the dtypes module and expose dtype constants
    let dtypes_module = crate::python_api::dtypes::make_dtypes_module(vm);
    // Expose all dtype constants directly on xos module
    if let Ok(dtype) = dtypes_module.get_attr("float16", vm) {
        module.set_attr("float16", dtype, vm).ok();
    }
    if let Ok(dtype) = dtypes_module.get_attr("float32", vm) {
        module.set_attr("float32", dtype.clone(), vm).ok();
        module.set_attr("float", dtype, vm).ok();
    }
    if let Ok(dtype) = dtypes_module.get_attr("float64", vm) {
        module.set_attr("float64", dtype, vm).ok();
    }
    if let Ok(dtype) = dtypes_module.get_attr("int8", vm) {
        module.set_attr("int8", dtype, vm).ok();
    }
    if let Ok(dtype) = dtypes_module.get_attr("int16", vm) {
        module.set_attr("int16", dtype, vm).ok();
    }
    if let Ok(dtype) = dtypes_module.get_attr("int32", vm) {
        module.set_attr("int32", dtype.clone(), vm).ok();
        module.set_attr("int", dtype, vm).ok();
    }
    if let Ok(dtype) = dtypes_module.get_attr("int64", vm) {
        module.set_attr("int64", dtype, vm).ok();
    }
    if let Ok(dtype) = dtypes_module.get_attr("uint8", vm) {
        module.set_attr("uint8", dtype, vm).ok();
    }
    if let Ok(dtype) = dtypes_module.get_attr("uint16", vm) {
        module.set_attr("uint16", dtype, vm).ok();
    }
    if let Ok(dtype) = dtypes_module.get_attr("uint32", vm) {
        module.set_attr("uint32", dtype, vm).ok();
    }
    if let Ok(dtype) = dtypes_module.get_attr("uint64", vm) {
        module.set_attr("uint64", dtype, vm).ok();
    }
    if let Ok(dtype) = dtypes_module.get_attr("bool", vm) {
        module.set_attr("bool", dtype, vm).ok();
    }
    
    // Define the Application base class in Python
    let application_class_code = crate::python_api::engine::pyapp::APPLICATION_CLASS_CODE;
    
    // Execute the Application class definition
    let scope = vm.new_scope_with_builtins();
    if let Err(e) = vm.run_code_string(scope.clone(), application_class_code, "<xos_module>".to_string()) {
        eprintln!("Failed to create Application class: {:?}", e);
    }
    
    // Add _ArrayWrapper and _ArrayResult to builtins so they're globally available
    if let Ok(array_wrapper) = scope.globals.get_item("_ArrayWrapper", vm) {
        vm.builtins.set_attr("_ArrayWrapper", array_wrapper, vm).ok();
    }
    if let Ok(array_result) = scope.globals.get_item("_ArrayResult", vm) {
        vm.builtins.set_attr("_ArrayResult", array_result, vm).ok();
    }
    
    // Get the Application class and _FrameWrapper from the scope and add them to the module
    if let Ok(app_class) = scope.globals.get_item("Application", vm) {
        module.set_attr("Application", app_class, vm).unwrap();
    }
    if let Ok(frame_wrapper) = scope.globals.get_item("_FrameWrapper", vm) {
        module.set_attr("_FrameWrapper", frame_wrapper.clone(), vm).unwrap();
        // Also add to builtins so pyapp can access it
        let _ = vm.builtins.set_attr("_FrameWrapper", frame_wrapper, vm);
    }
    
    module
}

