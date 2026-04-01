#[cfg(not(target_arch = "wasm32"))]
use pixels::{Pixels, PixelsBuilder, SurfaceTexture};
#[cfg(not(target_arch = "wasm32"))]
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, KeyCode, NamedKey, PhysicalKey},
    window::{CursorIcon, Fullscreen, Window, WindowAttributes, WindowId, WindowLevel},
};
#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
use winit::platform::windows::WindowExtWindows;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

use super::{
    f3_menu_handle_mouse_down, f3_menu_handle_mouse_move, f3_menu_handle_mouse_up, tick_f3_menu,
    F3Menu,
};
use super::engine::{
    tick_frame_delta, Application, CursorStyle, CursorStyleSetter, EngineState, FrameState,
    KeyboardState, MouseState, SafeRegionBoundingRectangle,
};
use crate::engine::keyboard::shortcuts::{
    NamedSpecialKey, PhysicalSpecialKey, SpecialKeyEvent,
};
use crate::rasterizer::RasterCache;

#[cfg(not(target_arch = "wasm32"))]
static SHOULD_EXIT: once_cell::sync::Lazy<Arc<AtomicBool>> = once_cell::sync::Lazy::new(|| Arc::new(AtomicBool::new(false)));

#[cfg(not(target_arch = "wasm32"))]
pub fn request_exit() {
    SHOULD_EXIT.store(true, Ordering::Relaxed);
}

/// How the native host creates the winit window: a normal windowed app vs floating overlay.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug)]
pub enum NativeLaunchMode {
    /// Default decorated window (`XOS Game`).
    Windowed,
    /// Borderless HUD: ~30% of primary monitor, top-left with a small margin, transparent, always on top.
    ///
    /// On Windows, the window is also hidden from the taskbar.
    Overlay,
}

/// ~30% of primary monitor size; inset from top-left by ~2% of the smaller monitor dimension (min 12px).
#[cfg(not(target_arch = "wasm32"))]
fn overlay_window_attributes(event_loop: &ActiveEventLoop) -> WindowAttributes {
    let (iw, ih, px, py) = event_loop
        .primary_monitor()
        .map(|m| {
            let pos = m.position();
            let sz = m.size();
            let margin =
                ((sz.width.min(sz.height) as f32) * 0.02).max(12.0).round() as i32;
            let inner_w = ((sz.width as f32) * 0.3).max(200.0).round() as u32;
            let inner_h = ((sz.height as f32) * 0.3).max(120.0).round() as u32;
            (inner_w, inner_h, pos.x + margin, pos.y + margin)
        })
        .unwrap_or((480, 320, 24, 24));

    Window::default_attributes()
        .with_title("xos overlay")
        .with_decorations(false)
        .with_transparent(true)
        .with_inner_size(PhysicalSize::new(iw, ih))
        .with_position(PhysicalPosition::new(px, py))
        .with_window_level(WindowLevel::AlwaysOnTop)
}

#[cfg(not(target_arch = "wasm32"))]
fn window_attributes_windowed() -> WindowAttributes {
    Window::default_attributes().with_title("XOS Game")
}

/// Windows often reports 0×0 when minimized. Keep the last non-zero size for buffers so the app
/// keeps simulating and `pixels` / `EngineState` stay the same length (avoids copy_from_slice panic).
#[cfg(not(target_arch = "wasm32"))]
#[inline]
fn physical_size_for_buffers(
    inner: winit::dpi::PhysicalSize<u32>,
    stored: winit::dpi::PhysicalSize<u32>,
) -> winit::dpi::PhysicalSize<u32> {
    if inner.width == 0 || inner.height == 0 {
        stored
    } else {
        inner
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct AppState {
    window: Window,
    pixels: Pixels<'static>,
    engine_state: EngineState,
    app: Box<dyn Application>,
    size: winit::dpi::PhysicalSize<u32>,
    raster_cache: RasterCache,
    last_tick_instant: Option<std::time::Instant>,
    // Modifier key tracking for shortcuts
    command_held: bool,
    shift_held: bool,
    alt_held: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl AppState {
    fn toggle_borderless_fullscreen(&self) {
        if self.window.fullscreen().is_some() {
            self.window.set_fullscreen(None);
        } else {
            self.window
                .set_fullscreen(Some(Fullscreen::Borderless(self.window.current_monitor())));
        }
    }

    fn render_pixels(&mut self) -> Result<(), pixels::Error> {
        self.pixels.render_with(|encoder, render_target, context| {
            crate::rasterizer::render_pending_gpu_passes(
                &mut self.raster_cache,
                encoder,
                &context.device,
                &context.queue,
                &context.texture,
                context.texture_extent,
                context.texture_format,
            );
            context.scaling_renderer.render(encoder, render_target);
            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        })
    }

    fn tick_and_render_frame(&mut self) {
        let expected_len = (self.size.width * self.size.height * 4) as usize;
        let mut mirror_ok = false;
        {
            let f = self.pixels.frame_mut();
            if f.len() == expected_len {
                unsafe {
                    self.engine_state
                        .frame
                        .tensor
                        .set_pixels_mirror_buffer(f.as_mut_ptr(), f.len());
                }
                mirror_ok = true;
            }
        }

        tick_frame_delta(&mut self.engine_state, &mut self.last_tick_instant);
        let _ = self.app.tick(&mut self.engine_state);

        {
            let width = self.size.width;
            let height = self.size.height;
            let mouse_x = self.engine_state.mouse.x;
            let mouse_y = self.engine_state.mouse.y;
            let safe_region = self.engine_state.frame.safe_region_boundaries.clone();
            let (buffer, keyboard) = {
                let buffer_ptr = self.engine_state.frame.buffer_mut() as *mut [u8];
                let keyboard_ptr: *mut crate::ui::onscreen_keyboard::OnScreenKeyboard =
                    &mut self.engine_state.keyboard.onscreen;
                (unsafe { &mut *buffer_ptr }, unsafe { &mut *keyboard_ptr })
            };
            keyboard.tick(buffer, width, height, mouse_x, mouse_y, &safe_region);
        }

        tick_f3_menu(&mut self.engine_state);

        if mirror_ok {
            self.engine_state.frame.tensor.clear_pixels_mirror_buffer();
            let _ = self.render_pixels();
        } else {
            let frame = self.pixels.frame_mut();
            let buffer = self.engine_state.frame_buffer_mut();
            if frame.len() == buffer.len() {
                frame.copy_from_slice(buffer);
                let _ = self.render_pixels();
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ApplicationHandler for AppState {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        // Application resumed
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        // Check if Ctrl+C was pressed
        if SHOULD_EXIT.load(Ordering::Relaxed) {
            event_loop.exit();
            return;
        }
        
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                let current_size =
                    physical_size_for_buffers(self.window.inner_size(), self.size);
                if current_size != self.size {
                    self.size = current_size;
                    let _ = self.pixels.resize_buffer(self.size.width, self.size.height);
                    let _ = self.pixels.resize_surface(self.size.width, self.size.height);
                    self.engine_state.resize_frame(self.size.width, self.size.height);
                    // Notify app of screen size change
                    let _ = self.app.on_screen_size_change(&mut self.engine_state, self.size.width, self.size.height);
                }
                self.tick_and_render_frame();
            }
            WindowEvent::Resized(new_size) => {
                if new_size.width == 0 || new_size.height == 0 {
                    self.window.request_redraw();
                } else {
                    self.size = new_size;
                    let _ = self.pixels.resize_buffer(self.size.width, self.size.height);
                    let _ = self.pixels.resize_surface(self.size.width, self.size.height);
                    self.engine_state.resize_frame(self.size.width, self.size.height);
                    let _ = self.app.on_screen_size_change(&mut self.engine_state, self.size.width, self.size.height);
                    // Keep simulation/render progressing during live drag-resize.
                    self.tick_and_render_frame();
                    self.window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor: _, .. } => {
                let new_size = physical_size_for_buffers(self.window.inner_size(), self.size);
                if new_size != self.size {
                    self.size = new_size;
                    let _ = self.pixels.resize_buffer(self.size.width, self.size.height);
                    let _ = self.pixels.resize_surface(self.size.width, self.size.height);
                    self.engine_state.resize_frame(self.size.width, self.size.height);
                    // Notify app of screen size change
                    let _ = self.app.on_screen_size_change(&mut self.engine_state, self.size.width, self.size.height);
                    self.window.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let prev_x = self.engine_state.mouse.x;
                let prev_y = self.engine_state.mouse.y;
            
                self.engine_state.mouse.x = position.x as f32;
                self.engine_state.mouse.y = position.y as f32;
            
                self.engine_state.mouse.dx = self.engine_state.mouse.x - prev_x;
                self.engine_state.mouse.dy = self.engine_state.mouse.y - prev_y;
            
                if !f3_menu_handle_mouse_move(&mut self.engine_state) {
                    let _ = self.app.on_mouse_move(&mut self.engine_state);
                }
            }
            WindowEvent::MouseInput {
                state: button_state,
                button: MouseButton::Left,
                ..
            } => match button_state {
                ElementState::Pressed => {
                    self.engine_state.mouse.is_left_clicking = true;
                    if !f3_menu_handle_mouse_down(&mut self.engine_state) {
                        let _ = self.app.on_mouse_down(&mut self.engine_state);
                    }
                }
                ElementState::Released => {
                    self.engine_state.mouse.is_left_clicking = false;
                    if !f3_menu_handle_mouse_up(&mut self.engine_state) {
                        let _ = self.app.on_mouse_up(&mut self.engine_state);
                    }
                }
            },
            WindowEvent::MouseInput {
                state: button_state,
                button: MouseButton::Right,
                ..
            } => match button_state {
                ElementState::Pressed => {
                    self.engine_state.mouse.is_right_clicking = true;
                }
                ElementState::Released => {
                    self.engine_state.mouse.is_right_clicking = false;
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(dx, dy) => (dx, dy),
                    MouseScrollDelta::PixelDelta(pos) => (pos.x as f32, pos.y as f32),
                };
                let _ = self.app.on_scroll(&mut self.engine_state, dx, dy);
            }
            WindowEvent::Ime(ime) => {
                match ime {
                    winit::event::Ime::Commit(text) => {
                        for ch in text.chars() {
                            let _ = self.app.on_key_char(&mut self.engine_state, ch);
                        }
                    }
                    winit::event::Ime::Preedit(_text, _) => {
                        // Handle preedit text if needed (for IME composition)
                        // For now, we can ignore it or handle it as needed
                    }
                    _ => {}
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                // Update modifier key states
                // On macOS, Super is Command; on Windows/Linux, Control is Ctrl
                #[cfg(target_os = "macos")]
                {
                    self.command_held = modifiers.state().super_key();
                }
                #[cfg(not(target_os = "macos"))]
                {
                    self.command_held = modifiers.state().control_key();
                }
                
                self.shift_held = modifiers.state().shift_key();
                self.alt_held = modifiers.state().alt_key();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    if matches!(event.logical_key, Key::Named(NamedKey::F3)) {
                        self.engine_state.f3_menu.toggle_visible();
                        return;
                    }

                    if self.alt_held
                        && self.shift_held
                        && matches!(event.physical_key, PhysicalKey::Code(KeyCode::KeyF))
                    {
                        self.toggle_borderless_fullscreen();
                        return;
                    }

                    let named_key = match event.logical_key {
                        Key::Named(NamedKey::Backspace) => Some(NamedSpecialKey::Backspace),
                        Key::Named(NamedKey::Enter) => Some(NamedSpecialKey::Enter),
                        Key::Named(NamedKey::Escape) => Some(NamedSpecialKey::Escape),
                        Key::Named(NamedKey::Tab) => Some(NamedSpecialKey::Tab),
                        Key::Named(NamedKey::ArrowLeft) => Some(NamedSpecialKey::ArrowLeft),
                        Key::Named(NamedKey::ArrowRight) => Some(NamedSpecialKey::ArrowRight),
                        Key::Named(NamedKey::ArrowUp) => Some(NamedSpecialKey::ArrowUp),
                        Key::Named(NamedKey::ArrowDown) => Some(NamedSpecialKey::ArrowDown),
                        _ => None,
                    };

                    let physical_key = match event.physical_key {
                        PhysicalKey::Code(KeyCode::Digit1) => Some(PhysicalSpecialKey::Digit1),
                        PhysicalKey::Code(KeyCode::Digit2) => Some(PhysicalSpecialKey::Digit2),
                        PhysicalKey::Code(KeyCode::Digit3) => Some(PhysicalSpecialKey::Digit3),
                        PhysicalKey::Code(KeyCode::KeyQ) => Some(PhysicalSpecialKey::KeyQ),
                        PhysicalKey::Code(KeyCode::KeyW) => Some(PhysicalSpecialKey::KeyW),
                        PhysicalKey::Code(KeyCode::KeyE) => Some(PhysicalSpecialKey::KeyE),
                        PhysicalKey::Code(KeyCode::KeyF) => Some(PhysicalSpecialKey::KeyF),
                        PhysicalKey::Code(KeyCode::KeyR) => Some(PhysicalSpecialKey::KeyR),
                        PhysicalKey::Code(KeyCode::KeyT) => Some(PhysicalSpecialKey::KeyT),
                        _ => None,
                    };

                    let character = if let Key::Character(ref s) = event.logical_key {
                        let mut chars = s.chars();
                        match (chars.next(), chars.next()) {
                            (Some(ch), None) => Some(ch),
                            _ => None,
                        }
                    } else {
                        None
                    };

                    self.app.on_special_key(
                        &mut self.engine_state,
                        SpecialKeyEvent {
                            named_key,
                            physical_key,
                            character,
                            command_held: self.command_held,
                            shift_held: self.shift_held,
                            alt_held: self.alt_held,
                        },
                    );

                    // Check if the event has text (for regular character input)
                    // In winit 0.30, text input should come through IME, but we can also
                    // check the text field as a fallback.
                    if let Some(text) = event.text.as_ref() {
                        for ch in text.chars() {
                            if !ch.is_control() || ch == '\n' || ch == '\t' || ch == '\r' {
                                let _ = self.app.on_key_char(&mut self.engine_state, ch);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Check if Ctrl+C was pressed
        if SHOULD_EXIT.load(Ordering::Relaxed) {
            event_loop.exit();
            return;
        }
        
        // Update cursor style
        match self.engine_state.mouse.style.get() {
            CursorStyle::Hidden => {
                self.window.set_cursor_visible(false);
            }
            other => {
                self.window.set_cursor_visible(true);
                let icon = match other {
                    CursorStyle::Default => CursorIcon::Default,
                    CursorStyle::Text => CursorIcon::Text,
                    CursorStyle::ResizeHorizontal => CursorIcon::EwResize,
                    CursorStyle::ResizeVertical => CursorIcon::NsResize,
                    CursorStyle::ResizeDiagonalNE => CursorIcon::NeswResize,
                    CursorStyle::ResizeDiagonalNW => CursorIcon::NwseResize,
                    CursorStyle::Hand => CursorIcon::Pointer,
                    CursorStyle::Crosshair => CursorIcon::Crosshair,
                    CursorStyle::Hidden => unreachable!(), // already handled above
                };
                self.window.set_cursor(icon);
            }
        }
        
        // Request continuous redraws to keep checking SHOULD_EXIT flag
        self.window.request_redraw();
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct AppStateWrapper {
    app_state: Option<AppState>,
    app: Box<dyn Application>,
    launch_mode: NativeLaunchMode,
}

#[cfg(not(target_arch = "wasm32"))]
impl ApplicationHandler for AppStateWrapper {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.app_state.is_none() {
            let attrs = match self.launch_mode {
                NativeLaunchMode::Windowed => window_attributes_windowed(),
                NativeLaunchMode::Overlay => overlay_window_attributes(event_loop),
            };
            let window = match event_loop.create_window(attrs) {
                Ok(w) => {
                    // Enable IME for text input
                    w.set_ime_allowed(true);
                    #[cfg(target_os = "windows")]
                    if matches!(self.launch_mode, NativeLaunchMode::Overlay) {
                        w.set_skip_taskbar(true);
                    }
                    w
                },
                Err(e) => {
                    eprintln!("Failed to create window: {}", e);
                    return;
                }
            };

            let size = window.inner_size();
            let surface_texture = SurfaceTexture::new(size.width, size.height, &window);
            let pixels = match PixelsBuilder::new(size.width, size.height, surface_texture)
                .enable_vsync(false)
                .build()
            {
                Ok(p) => unsafe { std::mem::transmute(p) }, // SAFETY: window outlives pixels
                Err(e) => {
                    eprintln!("Failed to create pixels: {}", e);
                    return;
                }
            };

            let safe_region = SafeRegionBoundingRectangle::full_screen();
            let mut engine_state = EngineState {
                frame: FrameState::new(size.width, size.height, safe_region),
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
                ui_scale_percent: 50,
                delta_time_seconds: 1.0 / 60.0,
            };

            if let Err(e) = self.app.setup(&mut engine_state) {
                eprintln!("Failed to setup app: {}", e);
                SHOULD_EXIT.store(true, Ordering::Relaxed);
                event_loop.exit();
                return;
            }

            let app = std::mem::replace(&mut self.app, Box::new(crate::apps::blank::BlankApp::new()));
            self.app_state = Some(AppState {
                window,
                pixels,
                engine_state,
                app,
                size,
                raster_cache: RasterCache::new(),
                last_tick_instant: None,
                command_held: false,
                shift_held: false,
                alt_held: false,
            });
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        if let Some(ref mut app_state) = self.app_state {
            app_state.window_event(event_loop, window_id, event);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(ref mut app_state) = self.app_state {
            app_state.about_to_wait(event_loop);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn run_native_event_loop(
    app: Box<dyn Application>,
    launch_mode: NativeLaunchMode,
) -> Result<(), Box<dyn std::error::Error>> {
    // Install Ctrl+C handler for clean shutdown
    let should_exit = SHOULD_EXIT.clone();
    ctrlc::set_handler(move || {
        println!("\nReceived Ctrl+C, shutting down gracefully...");
        should_exit.store(true, Ordering::Relaxed);
    })
    .expect("Error setting Ctrl+C handler");

    let event_loop = EventLoop::new().unwrap();

    let mut wrapper = AppStateWrapper {
        app_state: None,
        app,
        launch_mode,
    };

    event_loop.run_app(&mut wrapper)?;

    println!("Event loop exited cleanly");
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn start_native(app: Box<dyn Application>) -> Result<(), Box<dyn std::error::Error>> {
    run_native_event_loop(app, NativeLaunchMode::Windowed)
}

/// Headless host: runs setup/tick continuously without creating a window or renderer.
#[cfg(not(target_arch = "wasm32"))]
pub fn start_headless_native(
    mut app: Box<dyn Application>,
    width: u32,
    height: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    // Reset and install Ctrl+C handler for headless loop shutdown.
    SHOULD_EXIT.store(false, Ordering::Relaxed);
    let should_exit = SHOULD_EXIT.clone();
    ctrlc::set_handler(move || {
        should_exit.store(true, Ordering::Relaxed);
    })
    .map_err(|e| format!("Error setting Ctrl+C handler: {}", e))?;

    let safe_region = SafeRegionBoundingRectangle::full_screen();
    let mut engine_state = EngineState {
        frame: FrameState::new(width.max(1), height.max(1), safe_region),
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
        ui_scale_percent: 50,
        delta_time_seconds: 1.0 / 60.0,
    };

    if let Err(e) = app.setup(&mut engine_state) {
        return Err(format!("Failed to setup app: {}", e).into());
    }

    let mut last_tick_instant: Option<std::time::Instant> = None;
    while !SHOULD_EXIT.load(Ordering::Relaxed) {
        tick_frame_delta(&mut engine_state, &mut last_tick_instant);
        app.tick(&mut engine_state);
        tick_f3_menu(&mut engine_state);
    }
    SHOULD_EXIT.store(false, Ordering::Relaxed);
    Ok(())
}

/// Floating overlay host: same [`Application`] / [`EngineState`] as [`start_native`], but the
/// surface is a transparent top-left HUD (~30% of the primary monitor) and stays above normal windows.
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
pub fn start_overlay_native(app: Box<dyn Application>) -> Result<(), Box<dyn std::error::Error>> {
    run_native_event_loop(app, NativeLaunchMode::Overlay)
}
