use rustpython_vm::{PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs};
use std::collections::HashMap;
use std::io::Write;
use std::sync::{LazyLock, Mutex};
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
use std::time::Duration;

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
use pixels::{Pixels, PixelsBuilder, SurfaceTexture};
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, NamedKey},
    platform::pump_events::{EventLoopExtPumpEvents, PumpStatus},
    window::{CursorIcon, Window, WindowAttributes, WindowId},
};
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
use std::cell::RefCell;

static STANDALONE_FRAME_BUFFERS: LazyLock<Mutex<HashMap<u64, Vec<u8>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn standalone_frame_buffer_copy(viewport_id: u64) -> Option<Vec<u8>> {
    #[cfg(any(target_arch = "wasm32", target_os = "ios"))]
    {
        let _ = viewport_id;
        None
    }
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    {
        STANDALONE_FRAME_BUFFERS
            .lock()
            .ok()
            .and_then(|m| m.get(&viewport_id).cloned())
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
struct StandalonePreviewState {
    viewport_id: u64,
    window: Window,
    pixels: Pixels<'static>,
    size: PhysicalSize<u32>,
    has_presented_first_frame: bool,
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
struct StandalonePendingFrame {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
struct StandalonePreviewApp {
    states: HashMap<WindowId, StandalonePreviewState>,
    viewport_to_window: HashMap<u64, WindowId>,
    pending_frames: HashMap<u64, StandalonePendingFrame>,
    source_frames: HashMap<u64, StandalonePendingFrame>,
    paused_base_frames: HashMap<u64, StandalonePendingFrame>,
    pending_window_creates: HashMap<u64, (u32, u32)>,
    f3_engine_state: HashMap<u64, crate::engine::EngineState>,
    last_tick_instant: HashMap<u64, Option<std::time::Instant>>,
    command_held: HashMap<u64, bool>,
    shift_held: HashMap<u64, bool>,
    frame_pan_dragging: HashMap<u64, bool>,
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
impl StandalonePreviewApp {
    fn new() -> Self {
        Self {
            states: HashMap::new(),
            viewport_to_window: HashMap::new(),
            pending_frames: HashMap::new(),
            source_frames: HashMap::new(),
            paused_base_frames: HashMap::new(),
            pending_window_creates: HashMap::new(),
            f3_engine_state: HashMap::new(),
            last_tick_instant: HashMap::new(),
            command_held: HashMap::new(),
            shift_held: HashMap::new(),
            frame_pan_dragging: HashMap::new(),
        }
    }

    fn viewport_paused(&self, viewport_id: u64) -> bool {
        self.f3_engine_state
            .get(&viewport_id)
            .map(|es| es.paused)
            .unwrap_or(false)
    }

    fn ensure_windows_created(&mut self, event_loop: &ActiveEventLoop) {
        if self.pending_window_creates.is_empty() {
            return;
        }
        let pending: Vec<(u64, (u32, u32))> = self
            .pending_window_creates
            .iter()
            .map(|(id, size)| (*id, *size))
            .collect();
        for (viewport_id, (width, height)) in pending {
            if self.viewport_to_window.contains_key(&viewport_id) {
                self.pending_window_creates.remove(&viewport_id);
                continue;
            }
            let attrs = WindowAttributes::default()
                .with_title(format!("xos standalone preview ({viewport_id})"))
                .with_inner_size(PhysicalSize::new(width.max(1), height.max(1)))
                .with_visible(false);
            let Ok(window) = event_loop.create_window(attrs) else {
                continue;
            };
            let size = window.inner_size();
            let surface_texture = SurfaceTexture::new(size.width, size.height, &window);
            let Ok(pixels) = PixelsBuilder::new(size.width, size.height, surface_texture)
                .enable_vsync(false)
                .build()
            else {
                continue;
            };
            let pixels = unsafe { std::mem::transmute(pixels) };
            let window_id = window.id();
            self.states.insert(
                window_id,
                StandalonePreviewState {
                    viewport_id,
                    window,
                    pixels,
                    size,
                    has_presented_first_frame: false,
                },
            );
            self.viewport_to_window.insert(viewport_id, window_id);
            let safe_region = crate::engine::SafeRegionBoundingRectangle::full_screen();
            self.f3_engine_state.insert(
                viewport_id,
                crate::engine::EngineState {
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
                    overlay_red_pointer_enabled: false,
                    overlay_red_pointer_radius: 0.0,
                },
            );
            self.last_tick_instant.insert(viewport_id, None);
            self.command_held.insert(viewport_id, false);
            self.shift_held.insert(viewport_id, false);
            self.frame_pan_dragging.insert(viewport_id, false);
            self.pending_window_creates.remove(&viewport_id);
        }
    }

    fn render_viewport(&mut self, viewport_id: u64) {
        let paused = self.viewport_paused(viewport_id);
        let Some(window_id) = self.viewport_to_window.get(&viewport_id).copied() else {
            return;
        };
        let Some(state) = self.states.get_mut(&window_id) else {
            return;
        };
        let Some(frame_data) = self.pending_frames.get_mut(&viewport_id) else {
            return;
        };

        // Keep viewport/frame dimensions aligned to the current OS window size.
        // During interactive resize, Python frame production can lag by a tick.
        // Resizing pending frame data avoids oscillation and prevents apparent freezes.
        if frame_data.width != state.size.width || frame_data.height != state.size.height {
            let target_w = state.size.width.max(1);
            let target_h = state.size.height.max(1);
            let mut resized = vec![0u8; (target_w as usize)
                .saturating_mul(target_h as usize)
                .saturating_mul(4)];

            let copy_w = frame_data.width.min(target_w) as usize;
            let copy_h = frame_data.height.min(target_h) as usize;
            let src_stride = frame_data.width as usize * 4;
            let dst_stride = target_w as usize * 4;
            let row_bytes = copy_w * 4;
            for y in 0..copy_h {
                let src_off = y * src_stride;
                let dst_off = y * dst_stride;
                resized[dst_off..dst_off + row_bytes]
                    .copy_from_slice(&frame_data.rgba[src_off..src_off + row_bytes]);
            }

            frame_data.width = target_w;
            frame_data.height = target_h;
            frame_data.rgba = resized;
        }

        if !paused {
            if let Some(src) = self.source_frames.get(&viewport_id) {
                self.paused_base_frames.insert(
                    viewport_id,
                    StandalonePendingFrame {
                        width: src.width,
                        height: src.height,
                        rgba: src.rgba.clone(),
                    },
                );
            }
        }

        let (src_w, src_h, src_rgba) = if paused {
            if let Some(base) = self.paused_base_frames.get(&viewport_id) {
                (base.width, base.height, base.rgba.clone())
            } else {
                (frame_data.width, frame_data.height, frame_data.rgba.clone())
            }
        } else {
            if let Some(src) = self.source_frames.get(&viewport_id) {
                (src.width, src.height, src.rgba.clone())
            } else {
                (frame_data.width, frame_data.height, frame_data.rgba.clone())
            }
        };

        let expected = (state.size.width as usize)
            .saturating_mul(state.size.height as usize)
            .saturating_mul(4);
        if src_rgba.len() >= (src_w as usize).saturating_mul(src_h as usize).saturating_mul(4) {
            if let Some(es) = self.f3_engine_state.get_mut(&viewport_id) {
                let frame = es.frame.buffer_mut();
                frame.fill(0);
                let dst_w = state.size.width as usize;
                let dst_h = state.size.height as usize;
                let src_wu = src_w as usize;
                let src_hu = src_h as usize;
                let copy_w = src_wu.min(dst_w);
                let copy_h = src_hu.min(dst_h);
                let src_stride = src_wu * 4;
                let dst_stride = dst_w * 4;
                let row_bytes = copy_w * 4;
                for y in 0..copy_h {
                    let src_off = y * src_stride;
                    let dst_off = y * dst_stride;
                    frame[dst_off..dst_off + row_bytes]
                        .copy_from_slice(&src_rgba[src_off..src_off + row_bytes]);
                }

                if frame.len() == expected {
                    if es.paused {
                        if let Some(last) = self.last_tick_instant.get_mut(&viewport_id) {
                            *last = Some(std::time::Instant::now());
                        }
                    } else if let Some(last) = self.last_tick_instant.get_mut(&viewport_id) {
                        crate::engine::tick_frame_delta(es, last);
                    }
                    crate::engine::tick_frame_view_zoom(es);
                    crate::engine::apply_frame_view_zoom(es);
                    crate::engine::tick_f3_menu(es);
                    frame_data.rgba.copy_from_slice(es.frame.buffer_mut());
                }
            }
            state.pixels.frame_mut().copy_from_slice(&frame_data.rgba);
        }
        let _ = state.pixels.render();
        if !state.has_presented_first_frame {
            state.window.set_visible(true);
            state.has_presented_first_frame = true;
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
impl ApplicationHandler for StandalonePreviewApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.ensure_windows_created(event_loop);
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        let Some(viewport_id) = self
            .states
            .get(&_window_id)
            .map(|s| s.viewport_id)
        else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => {
                // Standalone preview is typically driven by user script loops
                // (e.g. `while ...: app.tick()`). Closing any preview window should
                // terminate the script immediately and close all windows together.
                let _ = viewport_id;
                std::process::exit(0);
            }
            WindowEvent::Resized(new_size) => {
                let Some(state) = self.states.get_mut(&_window_id) else { return; };
                if new_size.width > 0 && new_size.height > 0 {
                    state.size = new_size;
                    let _ = state.pixels.resize_buffer(new_size.width, new_size.height);
                    let _ = state.pixels.resize_surface(new_size.width, new_size.height);
                    if let Some(es) = self.f3_engine_state.get_mut(&viewport_id) {
                        es.resize_frame(new_size.width, new_size.height);
                        let _ = crate::engine::f3_menu_handle_mouse_move(es);
                    }
                    if let Some(frame_data) = self.pending_frames.get_mut(&viewport_id) {
                        let mut resized = vec![0u8; (new_size.width as usize)
                            .saturating_mul(new_size.height as usize)
                            .saturating_mul(4)];
                        let copy_w = frame_data.width.min(new_size.width) as usize;
                        let copy_h = frame_data.height.min(new_size.height) as usize;
                        let src_stride = frame_data.width as usize * 4;
                        let dst_stride = new_size.width as usize * 4;
                        let row_bytes = copy_w * 4;
                        for y in 0..copy_h {
                            let src_off = y * src_stride;
                            let dst_off = y * dst_stride;
                            resized[dst_off..dst_off + row_bytes]
                                .copy_from_slice(&frame_data.rgba[src_off..src_off + row_bytes]);
                        }
                        frame_data.width = new_size.width;
                        frame_data.height = new_size.height;
                        frame_data.rgba = resized;
                    }
                    state.window.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(es) = self.f3_engine_state.get_mut(&viewport_id) {
                    es.mouse.dx = position.x as f32 - es.mouse.x;
                    es.mouse.dy = position.y as f32 - es.mouse.y;
                    es.mouse.x = position.x as f32;
                    es.mouse.y = position.y as f32;

                    let command = *self.command_held.get(&viewport_id).unwrap_or(&false);
                    let shift = *self.shift_held.get(&viewport_id).unwrap_or(&false);
                    let dragging = *self.frame_pan_dragging.get(&viewport_id).unwrap_or(&false);

                    let actively_dragging = dragging
                        && es.mouse.is_left_clicking
                        && command
                        && shift;
                    if dragging && !actively_dragging {
                        self.frame_pan_dragging.insert(viewport_id, false);
                    }

                    if actively_dragging {
                        crate::engine::f3_menu_boost_interaction_fade(es);
                        if let Some(state) = self.states.get(&_window_id) {
                            crate::engine::frame_view_pan_by_pixels(
                                es,
                                es.mouse.dx,
                                es.mouse.dy,
                                state.size.width as f32,
                                state.size.height as f32,
                            );
                            state.window.set_cursor(CursorIcon::Grabbing);
                        }
                    } else {
                        let _ = crate::engine::f3_menu_handle_mouse_move(es);
                        if let Some(state) = self.states.get(&_window_id) {
                            if command && shift && es.frame_view_zoom > 1.001 {
                                state.window.set_cursor(CursorIcon::Grab);
                            } else {
                                state.window.set_cursor(CursorIcon::Default);
                            }
                        }
                    }
                }
                if let Some(state) = self.states.get(&_window_id) {
                    state.window.request_redraw();
                }
            }
            WindowEvent::MouseInput {
                state: button_state,
                button: MouseButton::Left,
                ..
            } => {
                if let Some(es) = self.f3_engine_state.get_mut(&viewport_id) {
                    match button_state {
                        ElementState::Pressed => {
                            es.mouse.is_left_clicking = true;
                            let command = *self.command_held.get(&viewport_id).unwrap_or(&false);
                            let shift = *self.shift_held.get(&viewport_id).unwrap_or(&false);
                            if command && shift && es.frame_view_zoom > 1.001 {
                                self.frame_pan_dragging.insert(viewport_id, true);
                                crate::engine::f3_menu_boost_interaction_fade(es);
                                if let Some(state) = self.states.get(&_window_id) {
                                    state.window.set_cursor(CursorIcon::Grabbing);
                                }
                            } else {
                                let was_paused = es.paused;
                                let _ = crate::engine::f3_menu_handle_mouse_down(es);
                                if !was_paused && es.paused {
                                    if let Some(frame_data) = self.pending_frames.get(&viewport_id) {
                                        self.paused_base_frames.insert(
                                            viewport_id,
                                            StandalonePendingFrame {
                                                width: frame_data.width,
                                                height: frame_data.height,
                                                rgba: frame_data.rgba.clone(),
                                            },
                                        );
                                    }
                                }
                            }
                        }
                        ElementState::Released => {
                            es.mouse.is_left_clicking = false;
                            if *self.frame_pan_dragging.get(&viewport_id).unwrap_or(&false) {
                                self.frame_pan_dragging.insert(viewport_id, false);
                            } else {
                                let _ = crate::engine::f3_menu_handle_mouse_up(es);
                            }
                        }
                    }
                }
                if let Some(state) = self.states.get(&_window_id) {
                    state.window.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed
                    && matches!(event.logical_key, Key::Named(NamedKey::F3))
                {
                    if let Some(es) = self.f3_engine_state.get_mut(&viewport_id) {
                        es.f3_menu.toggle_visible();
                    }
                    if let Some(state) = self.states.get(&_window_id) {
                        state.window.request_redraw();
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, dy) => dy,
                    MouseScrollDelta::PixelDelta(pos) => pos.y as f32,
                };
                let command = *self.command_held.get(&viewport_id).unwrap_or(&false);
                let shift = *self.shift_held.get(&viewport_id).unwrap_or(&false);
                if command {
                    if let Some(es) = self.f3_engine_state.get_mut(&viewport_id) {
                        let consumed = if shift {
                            crate::engine::f3_menu_handle_frame_zoom_scroll(es, dy)
                        } else {
                            crate::engine::f3_menu_handle_zoom_scroll(es, dy)
                        };
                        if consumed {
                            if let Some(state) = self.states.get(&_window_id) {
                                state.window.request_redraw();
                            }
                        }
                    }
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                #[cfg(target_os = "macos")]
                {
                    self.command_held
                        .insert(viewport_id, modifiers.state().super_key());
                }
                #[cfg(not(target_os = "macos"))]
                {
                    self.command_held
                        .insert(viewport_id, modifiers.state().control_key());
                }
                self.shift_held
                    .insert(viewport_id, modifiers.state().shift_key());
                if !(*self.command_held.get(&viewport_id).unwrap_or(&false)
                    && *self.shift_held.get(&viewport_id).unwrap_or(&false))
                {
                    self.frame_pan_dragging.insert(viewport_id, false);
                }
            }
            WindowEvent::RedrawRequested => {
                self.render_viewport(viewport_id);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.ensure_windows_created(event_loop);
        if self.states.is_empty() {
            event_loop.exit();
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

fn minecraft_color_to_ansi(code: char) -> Option<&'static str> {
    match code {
        '0' => Some("\x1b[30m"), // black
        '1' => Some("\x1b[34m"), // dark blue
        '2' => Some("\x1b[32m"), // dark green
        '3' => Some("\x1b[36m"), // dark aqua
        '4' => Some("\x1b[31m"), // dark red
        '5' => Some("\x1b[35m"), // dark purple
        '6' => Some("\x1b[33m"), // gold
        '7' => Some("\x1b[37m"), // gray
        '8' => Some("\x1b[90m"), // dark gray
        '9' => Some("\x1b[94m"), // blue
        'a' | 'A' => Some("\x1b[92m"), // green
        'b' | 'B' => Some("\x1b[96m"), // aqua
        'c' | 'C' => Some("\x1b[91m"), // red
        'd' | 'D' => Some("\x1b[95m"), // light purple
        'e' | 'E' => Some("\x1b[93m"), // yellow
        'f' | 'F' => Some("\x1b[97m"), // white
        'l' | 'L' => Some("\x1b[1m"),  // bold
        'n' | 'N' => Some("\x1b[4m"),  // underline
        'o' | 'O' => Some("\x1b[3m"),  // italic
        'm' | 'M' => Some("\x1b[9m"),  // strikethrough
        'r' | 'R' => Some("\x1b[0m"),  // reset
        _ => None,
    }
}

fn apply_minecraft_colors(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars().peekable();
    let mut used_ansi = false;

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if matches!(chars.peek(), Some('&')) {
                let _ = chars.next();
                out.push('&');
                continue;
            }
            out.push(ch);
            continue;
        }

        if ch == '&' {
            if matches!(chars.peek(), Some('&')) {
                let _ = chars.next();
                out.push('&');
                continue;
            }
            if let Some(code) = chars.next() {
                if let Some(ansi) = minecraft_color_to_ansi(code) {
                    out.push_str(ansi);
                    used_ansi = true;
                } else {
                    out.push('&');
                    out.push(code);
                }
                continue;
            }
            out.push('&');
            continue;
        }

        out.push(ch);
    }

    if used_ansi {
        out.push_str("\x1b[0m");
    }
    out
}

/// xos.colorize("&aHello &rworld") -> ANSI-colored string
fn xos_colorize(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let text: String = args.bind(vm)?;
    Ok(vm.ctx.new_str(apply_minecraft_colors(&text)).into())
}

/// xos.print_color("&4Error: &fdetails", end="\\n")
/// Supports Minecraft-style `&` codes and escapes (`\\&` or `&&`).
fn xos_print_color(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let text: String = if !args.args.is_empty() {
        args.args[0].clone().try_into_value(vm)?
    } else if let Some(t) = args.kwargs.get("text") {
        t.clone().try_into_value(vm)?
    } else {
        return Err(vm.new_type_error("print_color(text, end='\\n')".to_string()));
    };
    let end: String = if let Some(e) = args.kwargs.get("end") {
        e.clone().try_into_value(vm)?
    } else {
        "\n".to_string()
    };
    let rendered = apply_minecraft_colors(&text);
    print!("{rendered}{end}");
    let _ = std::io::stdout().flush();
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
    let viewport_id: u64 = if !args_vec.is_empty() {
        let id: i64 = args_vec[0].clone().try_into_value(vm)?;
        id.max(0) as u64
    } else {
        0
    };
    let width: usize = if args_vec.len() > 1 {
        let w: i32 = args_vec[1].clone().try_into_value(vm)?;
        w.max(1) as usize
    } else {
        800
    };
    let height: usize = if args_vec.len() > 2 {
        let h: i32 = args_vec[2].clone().try_into_value(vm)?;
        h.max(1) as usize
    } else {
        600
    };

    {
        let mut buffers = STANDALONE_FRAME_BUFFERS
            .lock()
            .map_err(|_| vm.new_runtime_error("standalone frame buffer lock poisoned".to_string()))?;
        let buf = buffers.entry(viewport_id).or_default();
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
    tensor_dict.set_item("_xos_viewport_id", vm.ctx.new_int(viewport_id as i64).into(), vm)?;

    let frame_dict = vm.ctx.new_dict();
    frame_dict.set_item("width", vm.ctx.new_int(width).into(), vm)?;
    frame_dict.set_item("height", vm.ctx.new_int(height).into(), vm)?;
    frame_dict.set_item("tensor", tensor_dict.into(), vm)?;
    Ok(frame_dict.into())
}

/// xos.frame._standalone_tensor_data(viewport_id) -> list[int]
/// Returns a flat copy of RGBA bytes for a standalone viewport buffer.
fn frame_standalone_tensor_data(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    #[cfg(any(target_arch = "wasm32", target_os = "ios"))]
    {
        Ok(vm.ctx.new_list(vec![]).into())
    }
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    {
        let args = _args;
        let viewport_id: u64 = if !args.args.is_empty() {
            let id: i64 = args.args[0].clone().try_into_value(vm)?;
            id.max(0) as u64
        } else {
            0
        };
        let bytes = STANDALONE_FRAME_BUFFERS
            .lock()
            .map_err(|_| vm.new_runtime_error("standalone frame buffer lock poisoned".to_string()))?
            .get(&viewport_id)
            .cloned()
            .unwrap_or_default();
        let py = bytes
            .into_iter()
            .map(|b| vm.ctx.new_int(b as i64).into())
            .collect();
        Ok(vm.ctx.new_list(py).into())
    }
}

/// xos.frame._end_standalone() - clears temporary standalone framebuffer context.
fn frame_end_standalone(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    crate::python_api::rasterizer::clear_frame_buffer_context();
    Ok(vm.ctx.none())
}

/// xos.frame._has_context() -> bool
/// Returns whether a framebuffer context is currently bound.
fn frame_has_context(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let has_context = crate::python_api::rasterizer::CURRENT_FRAME_BUFFER
        .lock()
        .map_err(|_| vm.new_runtime_error("frame buffer context lock poisoned".to_string()))?
        .is_some();
    Ok(vm.ctx.new_bool(has_context).into())
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
        let viewport_id: u64 = if !_args.args.is_empty() {
            let id: i64 = _args.args[0].clone().try_into_value(vm)?;
            id.max(0) as u64
        } else {
            0
        };
        let maybe_size = STANDALONE_PREVIEW_HOST.with(|slot| {
            slot.borrow()
                .as_ref()
                .and_then(|host| {
                    host.app
                        .viewport_to_window
                        .get(&viewport_id)
                        .and_then(|wid| host.app.states.get(wid))
                        .map(|s| (s.size.width, s.size.height))
                })
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

/// xos.frame._standalone_ui_scale() -> float | None
/// Current F3 UI scale as `ui_scale_percent / 100` from the standalone preview (Python-driven tick).
fn frame_standalone_ui_scale(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    #[cfg(any(target_arch = "wasm32", target_os = "ios"))]
    {
        Ok(vm.ctx.none())
    }
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    {
        let viewport_id: u64 = if !_args.args.is_empty() {
            let id: i64 = _args.args[0].clone().try_into_value(vm)?;
            id.max(0) as u64
        } else {
            0
        };
        let v = STANDALONE_PREVIEW_HOST.with(|slot| {
            slot.borrow().as_ref().and_then(|host| {
                host.app
                    .f3_engine_state
                    .get(&viewport_id)
                    .map(|es| es.ui_scale_percent as f64 / 100.0)
            })
        });
        if let Some(s) = v {
            Ok(vm.ctx.new_float(s).into())
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
        let viewport_id: u64 = if !_args.args.is_empty() {
            let id: i64 = _args.args[0].clone().try_into_value(vm)?;
            id.max(0) as u64
        } else {
            0
        };
        let width = *crate::python_api::rasterizer::CURRENT_FRAME_WIDTH.lock().unwrap() as u32;
        let height = *crate::python_api::rasterizer::CURRENT_FRAME_HEIGHT.lock().unwrap() as u32;
        let frame = STANDALONE_FRAME_BUFFERS
            .lock()
            .map_err(|_| vm.new_runtime_error("standalone frame buffer lock poisoned".to_string()))?
            .get(&viewport_id)
            .cloned()
            .unwrap_or_default();

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
                host.app.pending_frames.insert(
                    viewport_id,
                    StandalonePendingFrame {
                        width: width.max(1),
                        height: height.max(1),
                        rgba: frame,
                    },
                );
                if let Some(pf) = host.app.pending_frames.get(&viewport_id) {
                    host.app.source_frames.insert(
                        viewport_id,
                        StandalonePendingFrame {
                            width: pf.width,
                            height: pf.height,
                            rgba: pf.rgba.clone(),
                        },
                    );
                    if host.app.viewport_paused(viewport_id) {
                        host.app.paused_base_frames.insert(
                            viewport_id,
                            StandalonePendingFrame {
                                width: pf.width,
                                height: pf.height,
                                rgba: pf.rgba.clone(),
                            },
                        );
                    }
                }
                if !host.app.viewport_to_window.contains_key(&viewport_id) {
                    host.app
                        .pending_window_creates
                        .insert(viewport_id, (width.max(1), height.max(1)));
                }
                host.app.render_viewport(viewport_id);
                loop {
                    let timeout = if host.app.viewport_paused(viewport_id) {
                        Some(Duration::from_millis(250))
                    } else {
                        Some(Duration::ZERO)
                    };
                    match host.event_loop.pump_app_events(timeout, &mut host.app) {
                        PumpStatus::Continue => {}
                        PumpStatus::Exit(_) => {
                            // Keep EventLoop alive: many platforms allow only one per process.
                        }
                    }
                    let mut stepped = false;
                    if let Some(es) = host.app.f3_engine_state.get_mut(&viewport_id) {
                        if es.paused && es.pending_step_ticks > 0 {
                            es.pending_step_ticks = es.pending_step_ticks.saturating_sub(1);
                            stepped = true;
                        }
                    }
                    if stepped {
                        break;
                    }
                    if !host.app.viewport_paused(viewport_id) {
                        break;
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
    module
        .set_attr("colorize", vm.new_function("colorize", xos_colorize), vm)
        .unwrap();
    module
        .set_attr("print_color", vm.new_function("print_color", xos_print_color), vm)
        .unwrap();
    
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
        .set_attr("_has_context", vm.new_function("_has_context", frame_has_context), vm)
        .unwrap();
    frame_module
        .set_attr(
            "_standalone_window_size",
            vm.new_function("_standalone_window_size", frame_standalone_window_size),
            vm,
        )
        .unwrap();
    frame_module
        .set_attr(
            "_standalone_ui_scale",
            vm.new_function("_standalone_ui_scale", frame_standalone_ui_scale),
            vm,
        )
        .unwrap();
    frame_module
        .set_attr("_present_standalone", vm.new_function("_present_standalone", frame_present_standalone), vm)
        .unwrap();
    frame_module
        .set_attr(
            "_standalone_tensor_data",
            vm.new_function("_standalone_tensor_data", frame_standalone_tensor_data),
            vm,
        )
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

    // Add terminal helpers for terminal-aware Python apps.
    let terminal_module = crate::python_api::terminal::make_terminal_module(vm);
    module.set_attr("terminal", terminal_module, vm).unwrap();

    // Add process manager helpers.
    let manager_module = crate::python_api::manager::make_manager_module(vm);
    module.set_attr("manager", manager_module, vm).unwrap();

    // Add auth helpers.
    let auth_module = crate::python_api::auth::make_auth_module(vm);
    module.set_attr("auth", auth_module, vm).unwrap();

    // Add AI helpers.
    let ai_module = crate::python_api::ai::make_ai_module(vm);
    module.set_attr("ai", ai_module, vm).unwrap();
    
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
    let tensors_module = crate::python_api::make_tensors_module(vm);
    module.set_attr("tensor", tensors_module.get_attr("tensor", vm).unwrap(), vm).unwrap();
    module.set_attr("zeros", tensors_module.get_attr("zeros", vm).unwrap(), vm).unwrap();
    module.set_attr("ones", tensors_module.get_attr("ones", vm).unwrap(), vm).unwrap();
    module.set_attr("full", tensors_module.get_attr("full", vm).unwrap(), vm).unwrap();
    module.set_attr("arange", tensors_module.get_attr("arange", vm).unwrap(), vm).unwrap();
    module.set_attr("stack", tensors_module.get_attr("stack", vm).unwrap(), vm).unwrap();
    module.set_attr("where", tensors_module.get_attr("where", vm).unwrap(), vm).unwrap();
    module.set_attr("clip", tensors_module.get_attr("clip", vm).unwrap(), vm).unwrap();

    crate::python_api::burn_train::register_burn_module(&module, vm);

    // Add nn submodule
    let nn_module = crate::python_api::nn::make_nn_module(vm);
    module.set_attr("nn", nn_module, vm).unwrap();
    
    // Add the data submodule
    let data_module = crate::python_api::data::make_data_module(vm);
    module.set_attr("data", data_module, vm).unwrap();

    let path_module = crate::python_api::path::make_path_module(vm);
    module.set_attr("path", path_module, vm).unwrap();

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
    
    // xos.Tensor — single Python tensor type (see APPLICATION_CLASS_CODE)
    if let Ok(tensor_cls) = scope.globals.get_item("Tensor", vm) {
        vm.builtins.set_attr("Tensor", tensor_cls.clone(), vm).ok();
        module.set_attr("Tensor", tensor_cls, vm).ok();
    }

    if let Ok(app_class) = scope.globals.get_item("Application", vm) {
        module.set_attr("Application", app_class, vm).unwrap();
    }
    if let Ok(frame_cls) = scope.globals.get_item("Frame", vm) {
        module.set_attr("Frame", frame_cls.clone(), vm).unwrap();
        let _ = vm.builtins.set_attr("Frame", frame_cls, vm);
    }

    crate::python_api::mesh::register_mesh(&module, vm);
    crate::python_api::mouse::register_mouse(&module, vm);

    module
}

