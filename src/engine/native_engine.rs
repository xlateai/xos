#[cfg(not(target_arch = "wasm32"))]
use pixels::{Pixels, SurfaceTexture};
#[cfg(not(target_arch = "wasm32"))]
use winit::{
    event::*,
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use super::engine::{Application, EngineState, FrameState, MouseState, KeyboardState};

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
        keyboard: KeyboardState::new(),
    };

    app.setup(&mut engine_state)?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        match event {
            Event::RedrawRequested(_) => {
                let current_size = window.inner_size();
                if current_size != size {
                    size = current_size;
                    let _ = pixels.resize_buffer(size.width, size.height);
                    let _ = pixels.resize_surface(size.width, size.height);
                    engine_state.frame.width = size.width;
                    engine_state.frame.height = size.height;
                    engine_state.frame.buffer = vec![0; (size.width * size.height * 4) as usize];
                }

                engine_state.frame.buffer.fill(0);
                app.tick(&mut engine_state);

                let frame = pixels.frame_mut();
                if frame.len() == engine_state.frame.buffer.len() {
                    frame.copy_from_slice(&engine_state.frame.buffer);
                    let _ = pixels.render();
                } else {
                    engine_state.frame.buffer.resize(frame.len(), 0);
                    eprintln!("Buffer size mismatch detected and fixed. New size: {}", frame.len());
                }
            }
            Event::MainEventsCleared => {
                window.request_redraw();
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                WindowEvent::Resized(new_size) | WindowEvent::ScaleFactorChanged { new_inner_size: &mut new_size, .. } => {
                    size = new_size;
                    let _ = pixels.resize_buffer(size.width, size.height);
                    let _ = pixels.resize_surface(size.width, size.height);
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
                WindowEvent::MouseInput { state: button_state, button: MouseButton::Left, .. } => {
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
                WindowEvent::MouseWheel { delta, .. } => {
                    let (dx, dy) = match delta {
                        MouseScrollDelta::LineDelta(dx, dy) => (dx, dy),
                        MouseScrollDelta::PixelDelta(pos) => (pos.x as f32, pos.y as f32),
                    };
                    app.on_scroll(&mut engine_state, dx, dy);
                }
                WindowEvent::ReceivedCharacter(ch) => {
                    app.on_key_char(&mut engine_state, ch);
                }
                WindowEvent::KeyboardInput {
                    input: KeyboardInput {
                        state: ElementState::Pressed,
                        virtual_keycode: Some(VirtualKeyCode::Back),
                        ..
                    },
                    ..
                } => {
                    app.on_key_char(&mut engine_state, '\u{8}');
                }
                _ => {}
            },
            _ => {}
        }
    });
}