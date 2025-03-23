// use pixels::{Pixels, SurfaceTexture};
// use std::time::{Duration, Instant};
// use winit::{
//     dpi::LogicalSize,
//     event::{ElementState, Event, MouseButton, WindowEvent},
//     event_loop::{ControlFlow, EventLoop},
//     window::WindowBuilder,
// };

// const WIDTH: u32 = 256;
// const HEIGHT: u32 = 256;
// const TPS: f32 = 144.0;
// const MIN_RADIUS: f32 = 10.0;
// const MAX_RADIUS: f32 = 20.0;
// const TRANSITION_SPEED: f32 = 30.0; // Units per second

// struct GameState {
//     radius: f32,
//     is_clicked: bool,
//     last_tick: Instant,
// }

// impl GameState {
//     fn new() -> Self {
//         Self {
//             radius: MIN_RADIUS,
//             is_clicked: false,
//             last_tick: Instant::now(),
//         }
//     }

//     fn tick(&mut self) {
//         let now = Instant::now();
//         let delta_time = now.duration_since(self.last_tick).as_secs_f32();
//         self.last_tick = now;

//         // Update radius based on click state
//         if self.is_clicked {
//             self.radius += TRANSITION_SPEED * delta_time;
//             if self.radius > MAX_RADIUS {
//                 self.radius = MAX_RADIUS;
//             }
//         } else {
//             self.radius -= TRANSITION_SPEED * delta_time;
//             if self.radius < MIN_RADIUS {
//                 self.radius = MIN_RADIUS;
//             }
//         }
//     }

//     fn draw(&self, frame: &mut [u8]) {
//         // Clear the frame with black
//         for pixel in frame.chunks_exact_mut(4) {
//             pixel[0] = 0x00; // R
//             pixel[1] = 0x00; // G
//             pixel[2] = 0x00; // B
//             pixel[3] = 0xff; // A
//         }

//         // Draw a green circle in the center
//         let center_x = WIDTH as f32 / 2.0;
//         let center_y = HEIGHT as f32 / 2.0;
//         let radius_squared = self.radius * self.radius;

//         for y in 0..HEIGHT {
//             for x in 0..WIDTH {
//                 let dx = x as f32 - center_x;
//                 let dy = y as f32 - center_y;
//                 let distance_squared = dx * dx + dy * dy;

//                 if distance_squared <= radius_squared {
//                     let index = ((y * WIDTH + x) as usize) * 4;
//                     frame[index] = 0x00;     // R
//                     frame[index + 1] = 0xff; // G
//                     frame[index + 2] = 0x00; // B
//                     frame[index + 3] = 0xff; // A
//                 }
//             }
//         }
//     }
// }

// pub fn open_viewport() {
//     // Create the event loop and window
//     let event_loop = EventLoop::new();
//     let window = WindowBuilder::new()
//         .with_title("XOS Viewport")
//         .with_inner_size(LogicalSize::new(WIDTH, HEIGHT))
//         .with_resizable(false)
//         .build(&event_loop)
//         .unwrap();

//     // Create the pixel buffer
//     let size = window.inner_size(); // This gives you the physical pixel size (needed for macos)
//     let surface_texture = SurfaceTexture::new(size.width, size.height, &window);
//     let mut pixels = Pixels::new(WIDTH, HEIGHT, surface_texture).unwrap();

//     // Create game state
//     let mut game_state = GameState::new();
//     let mut last_tick = Instant::now();
//     let tick_duration = Duration::from_secs_f32(1.0 / TPS);

//     // Run the event loop
//     event_loop.run(move |event, _, control_flow| {
//         match event {
//             Event::WindowEvent {
//                 event: WindowEvent::CloseRequested,
//                 ..
//             } => {
//                 *control_flow = ControlFlow::Exit;
//             }
//             Event::WindowEvent {
//                 event: WindowEvent::MouseInput { state, button: MouseButton::Left, .. },
//                 ..
//             } => {
//                 match state {
//                     ElementState::Pressed => game_state.is_clicked = true,
//                     ElementState::Released => game_state.is_clicked = false,
//                 }
//             }
//             Event::RedrawRequested(_) => {
//                 // Render the current state
//                 game_state.draw(pixels.frame_mut());
                
//                 if let Err(e) = pixels.render() {
//                     eprintln!("pixels.render() failed: {}", e);
//                     *control_flow = ControlFlow::Exit;
//                 }
//             }
//             Event::MainEventsCleared => {
//                 // Check if we need to run a tick
//                 let now = Instant::now();
//                 if now.duration_since(last_tick) >= tick_duration {
//                     game_state.tick();
//                     last_tick = now;
//                 }

//                 // Request redraw to show current state
//                 window.request_redraw();
                
//                 // Instead of waiting, we'll poll so the animation remains smooth
//                 *control_flow = ControlFlow::Poll;
//             }
//             _ => (),
//         }
//     });
// }