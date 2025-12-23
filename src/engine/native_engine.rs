#[cfg(not(target_arch = "wasm32"))]
use pixels::{Pixels, SurfaceTexture};
#[cfg(not(target_arch = "wasm32"))]
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, NamedKey},
    window::{CursorIcon, Window, WindowId},
};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

use super::engine::{Application, EngineState, KeyboardState, MouseState, CursorStyle, CursorStyleSetter, FrameState, SafeRegionBoundingRectangle};
use crate::keyboard::shortcuts::detect_shortcut;

#[cfg(not(target_arch = "wasm32"))]
static SHOULD_EXIT: once_cell::sync::Lazy<Arc<AtomicBool>> = once_cell::sync::Lazy::new(|| Arc::new(AtomicBool::new(false)));

#[cfg(not(target_arch = "wasm32"))]
struct AppState {
    window: Window,
    pixels: Pixels<'static>,
    engine_state: EngineState,
    app: Box<dyn Application>,
    size: winit::dpi::PhysicalSize<u32>,
    // Modifier key tracking for shortcuts
    command_held: bool,
    shift_held: bool,
    alt_held: bool,
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
                let current_size = self.window.inner_size();
                if current_size != self.size {
                    self.size = current_size;
                    let _ = self.pixels.resize_buffer(self.size.width, self.size.height);
                    let _ = self.pixels.resize_surface(self.size.width, self.size.height);
                    self.engine_state.resize_frame(self.size.width, self.size.height);
                }
                
                // Tick the app first
                let _ = self.app.tick(&mut self.engine_state);
                
                // Then draw the keyboard on top (handles positioning, rendering, and key repeats)
                {
                    let width = self.size.width;
                    let height = self.size.height;
                    let mouse_x = self.engine_state.mouse.x;
                    let mouse_y = self.engine_state.mouse.y;
                    let safe_region = self.engine_state.frame.safe_region_boundaries.clone();
                    // Split borrows: get buffer and keyboard separately through engine_state
                    let (buffer, keyboard) = {
                        let buffer_ptr = self.engine_state.frame.buffer_mut() as *mut [u8];
                        let keyboard_ptr: *mut crate::text::onscreen_keyboard::OnScreenKeyboard = &mut self.engine_state.keyboard.onscreen;
                        (unsafe { &mut *buffer_ptr }, unsafe { &mut *keyboard_ptr })
                    };
                    keyboard.tick(buffer, width, height, mouse_x, mouse_y, &safe_region);
                }

                let frame = self.pixels.frame_mut();
                let buffer = self.engine_state.frame_buffer_mut();
                if frame.len() == buffer.len() {
                    frame.copy_from_slice(buffer);
                    let _ = self.pixels.render();
                } else {
                    // Resize if there's a mismatch
                    self.engine_state.resize_frame(self.size.width, self.size.height);
                    let buffer = self.engine_state.frame_buffer_mut();
                    frame.copy_from_slice(buffer);
                    eprintln!("Buffer size mismatch detected and fixed. New size: {}", frame.len());
                }
            }
            WindowEvent::Resized(new_size) => {
                self.size = new_size;
                let _ = self.pixels.resize_buffer(self.size.width, self.size.height);
                let _ = self.pixels.resize_surface(self.size.width, self.size.height);
                self.engine_state.resize_frame(self.size.width, self.size.height);
                self.window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor: _, .. } => {
                let new_size = self.window.inner_size();
                if new_size != self.size {
                    self.size = new_size;
                    let _ = self.pixels.resize_buffer(self.size.width, self.size.height);
                    let _ = self.pixels.resize_surface(self.size.width, self.size.height);
                    self.engine_state.resize_frame(self.size.width, self.size.height);
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
            
                let _ = self.app.on_mouse_move(&mut self.engine_state);
            }
            WindowEvent::MouseInput {
                state: button_state,
                button: MouseButton::Left,
                ..
            } => match button_state {
                ElementState::Pressed => {
                    self.engine_state.mouse.is_left_clicking = true;
                    let _ = self.app.on_mouse_down(&mut self.engine_state);
                }
                ElementState::Released => {
                    self.engine_state.mouse.is_left_clicking = false;
                    let _ = self.app.on_mouse_up(&mut self.engine_state);
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
                    // Handle special keys
                    match event.logical_key {
                        Key::Named(NamedKey::Backspace) => {
                            let _ = self.app.on_key_char(&mut self.engine_state, '\u{8}');
                        }
                        Key::Named(NamedKey::Enter) => {
                            let _ = self.app.on_key_char(&mut self.engine_state, '\n');
                        }
                        Key::Named(NamedKey::Tab) => {
                            let _ = self.app.on_key_char(&mut self.engine_state, '\t');
                        }
                        Key::Named(NamedKey::ArrowLeft) => {
                            let _ = self.app.on_key_char(&mut self.engine_state, '\u{2190}'); // ←
                        }
                        Key::Named(NamedKey::ArrowRight) => {
                            let _ = self.app.on_key_char(&mut self.engine_state, '\u{2192}'); // →
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            let _ = self.app.on_key_char(&mut self.engine_state, '\u{2191}'); // ↑
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            let _ = self.app.on_key_char(&mut self.engine_state, '\u{2193}'); // ↓
                        }
                        _ => {
                            // Check if the event has text (for regular character input)
                            // In winit 0.30, text input should come through IME, but we can also
                            // check the text field as a fallback
                            if let Some(text) = event.text.as_ref() {
                                for ch in text.chars() {
                                    // Check for keyboard shortcuts first (desktop only, not iOS)
                                    #[cfg(not(target_os = "ios"))]
                                    if let Some(shortcut) = detect_shortcut(ch, self.command_held, self.shift_held) {
                                        self.app.on_key_shortcut(&mut self.engine_state, shortcut);
                                        continue; // Don't process as regular character
                                    }
                                    
                                    if !ch.is_control() || ch == '\n' || ch == '\t' || ch == '\r' {
                                        let _ = self.app.on_key_char(&mut self.engine_state, ch);
                                    }
                                }
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
        
        self.window.request_redraw();
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct AppStateWrapper {
    app_state: Option<AppState>,
    app: Box<dyn Application>,
}

#[cfg(not(target_arch = "wasm32"))]
impl ApplicationHandler for AppStateWrapper {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.app_state.is_none() {
            let window = match event_loop.create_window(Window::default_attributes().with_title("XOS Game")) {
                Ok(w) => {
                    // Enable IME for text input
                    w.set_ime_allowed(true);
                    w
                },
                Err(e) => {
                    eprintln!("Failed to create window: {}", e);
                    return;
                }
            };

            let size = window.inner_size();
            let surface_texture = SurfaceTexture::new(size.width, size.height, &window);
            let pixels = match Pixels::new(size.width, size.height, surface_texture) {
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
                    onscreen: crate::text::onscreen_keyboard::OnScreenKeyboard::new(),
                },
            };

            if let Err(e) = self.app.setup(&mut engine_state) {
                eprintln!("Failed to setup app: {}", e);
                return;
            }

            let app = std::mem::replace(&mut self.app, Box::new(crate::apps::blank::BlankApp::new()));
            self.app_state = Some(AppState {
                window,
                pixels,
                engine_state,
                app,
                size,
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
pub fn start_native(app: Box<dyn Application>) -> Result<(), Box<dyn std::error::Error>> {
    // Install Ctrl+C handler for clean shutdown
    let should_exit = SHOULD_EXIT.clone();
    ctrlc::set_handler(move || {
        println!("\nReceived Ctrl+C, shutting down gracefully...");
        should_exit.store(true, Ordering::Relaxed);
    }).expect("Error setting Ctrl+C handler");
    
    let event_loop = EventLoop::new().unwrap();
    
    let mut wrapper = AppStateWrapper {
        app_state: None,
        app,
    };
    
    event_loop.run_app(&mut wrapper)?;
    Ok(())
}
