// use pixels::{Pixels, SurfaceTexture};
// use winit::{
//     dpi::LogicalSize,
//     event::{Event, WindowEvent},
//     event_loop::{ControlFlow, EventLoop},
//     window::WindowBuilder,
// };

// const WIDTH: u32 = 256;
// const HEIGHT: u32 = 256;

// pub fn open_window() {
//     // Create the event loop and window
//     let event_loop = EventLoop::new();
//     let window = WindowBuilder::new()
//         .with_title("Single White Pixel")
//         .with_inner_size(LogicalSize::new(WIDTH, HEIGHT))
//         .with_resizable(false)
//         .build(&event_loop)
//         .unwrap();

//     // Create the pixel buffer
//     let surface_texture = SurfaceTexture::new(WIDTH, HEIGHT, &window);
//     let mut pixels = Pixels::new(WIDTH, HEIGHT, surface_texture).unwrap();

//     // Run the event loop
//     event_loop.run(move |event, _, control_flow| {
//         *control_flow = ControlFlow::Wait;

//         match event {
//             Event::WindowEvent {
//                 event: WindowEvent::CloseRequested,
//                 ..
//             } => {
//                 *control_flow = ControlFlow::Exit;
//             }
//             Event::RedrawRequested(_) => {
//                 let frame = pixels.frame_mut();
                
//                 // Fill the entire frame with black pixels
//                 for pixel in frame.chunks_exact_mut(4) {
//                     pixel[0] = 0x00; // R
//                     pixel[1] = 0x00; // G
//                     pixel[2] = 0x00; // B
//                     pixel[3] = 0xff; // A
//                 }
                
//                 // Set the pixel at (128, 128) to white
//                 let index = ((128 * WIDTH as usize) + 128) * 4;
//                 frame[index] = 0xff;     // R
//                 frame[index + 1] = 0xff; // G
//                 frame[index + 2] = 0xff; // B
//                 frame[index + 3] = 0xff; // A
                
//                 // Render the frame
//                 if let Err(e) = pixels.render() {
//                     eprintln!("pixels.render() failed: {}", e);
//                     *control_flow = ControlFlow::Exit;
//                 }
//             }
//             Event::MainEventsCleared => {
//                 window.request_redraw();
//             }
//             _ => (),
//         }
//     });
// }