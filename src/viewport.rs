use pixels::{Pixels, SurfaceTexture};
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, MouseButton, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use cpal::traits::{DeviceTrait, HostTrait};

const WIDTH: u32 = 256;
const HEIGHT: u32 = 256;
const TPS: f32 = 144.0;
const MIN_RADIUS: f32 = 10.0;
const MAX_RADIUS: f32 = 20.0;
const TRANSITION_SPEED: f32 = 30.0; // Units per second

struct GameState {
    radius: f32,
    is_clicked: bool,
    last_tick: Instant,
    audio_level: Arc<Mutex<f32>>,
}

impl GameState {
    fn new() -> Self {
        Self {
            radius: MIN_RADIUS,
            is_clicked: false,
            last_tick: Instant::now(),
            audio_level: Arc::new(Mutex::new(0.0)),
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let delta_time = now.duration_since(self.last_tick).as_secs_f32();
        self.last_tick = now;

        // Update radius based on click state
        if self.is_clicked {
            self.radius += TRANSITION_SPEED * delta_time;
            if self.radius > MAX_RADIUS {
                self.radius = MAX_RADIUS;
            }
        } else {
            self.radius -= TRANSITION_SPEED * delta_time;
            if self.radius < MIN_RADIUS {
                self.radius = MIN_RADIUS;
            }
        }

        // Print the current audio level
        if let Ok(level) = self.audio_level.lock() {
            println!("The audio from microphone is this loud: {:.6}", *level);
        }
    }

    fn draw(&self, frame: &mut [u8]) {
        // Clear the frame with black
        for pixel in frame.chunks_exact_mut(4) {
            pixel[0] = 0x00; // R
            pixel[1] = 0x00; // G
            pixel[2] = 0x00; // B
            pixel[3] = 0xff; // A
        }

        // Draw a green circle in the center
        let center_x = WIDTH as f32 / 2.0;
        let center_y = HEIGHT as f32 / 2.0;
        let radius_squared = self.radius * self.radius;

        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let dx = x as f32 - center_x;
                let dy = y as f32 - center_y;
                let distance_squared = dx * dx + dy * dy;

                if distance_squared <= radius_squared {
                    let index = ((y * WIDTH + x) as usize) * 4;
                    frame[index] = 0x00;     // R
                    frame[index + 1] = 0xff; // G
                    frame[index + 2] = 0x00; // B
                    frame[index + 3] = 0xff; // A
                }
            }
        }
    }
}

pub fn open_viewport() {
    // Create the event loop and window
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("XOS Viewport with Audio")
        .with_inner_size(LogicalSize::new(WIDTH, HEIGHT))
        .with_resizable(false)
        .build(&event_loop)
        .unwrap();

    // Create the pixel buffer
    let surface_texture = SurfaceTexture::new(WIDTH, HEIGHT, &window);
    let mut pixels = Pixels::new(WIDTH, HEIGHT, surface_texture).unwrap();

    // Create game state
    let mut game_state = GameState::new();
    let mut last_tick = Instant::now();
    let tick_duration = Duration::from_secs_f32(1.0 / TPS);

    // Try to set up audio capture
    let audio_level = game_state.audio_level.clone();
    
    // Print audio device information
    let host = cpal::default_host();
    println!("Audio host: {:?}", host.id());
    
    println!("Available input devices:");
    match host.input_devices() {
        Ok(devices) => {
            let mut found_device = false;
            for device in devices {
                if let Ok(name) = device.name() {
                    println!("  - {}", name);
                    found_device = true;
                }
            }
            if !found_device {
                println!("  No input devices found");
            }
        },
        Err(e) => {
            println!("Error getting input devices: {}", e);
        }
    }

    // Start audio capture in a separate thread to avoid blocking the main loop
    std::thread::spawn(move || {
        // Get the default input device
        let host = cpal::default_host();
        let device = match host.default_input_device() {
            Some(d) => d,
            None => {
                println!("No default input device found");
                return;
            }
        };

        println!("Using input device: {}", device.name().unwrap_or_else(|_| "unknown".to_string()));

        // Get input config
        let config = match device.default_input_config() {
            Ok(c) => c,
            Err(e) => {
                println!("Error getting default input config: {}", e);
                return;
            }
        };

        println!("Using config: {:?}", config);

        // Create an input stream
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => create_input_stream::<f32>(&device, &config.into(), audio_level),
            cpal::SampleFormat::I16 => create_input_stream::<i16>(&device, &config.into(), audio_level),
            cpal::SampleFormat::U16 => create_input_stream::<u16>(&device, &config.into(), audio_level),
        };

        match stream {
            Ok(_) => println!("Audio stream started successfully"),
            Err(e) => println!("Error starting audio stream: {}", e),
        }

        // This thread needs to keep running for the audio to work
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    });

    // Run the event loop
    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            Event::WindowEvent {
                event: WindowEvent::MouseInput { state, button: MouseButton::Left, .. },
                ..
            } => {
                match state {
                    ElementState::Pressed => game_state.is_clicked = true,
                    ElementState::Released => game_state.is_clicked = false,
                }
            }
            Event::RedrawRequested(_) => {
                // Render the current state
                game_state.draw(pixels.frame_mut());
                
                if let Err(e) = pixels.render() {
                    eprintln!("pixels.render() failed: {}", e);
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::MainEventsCleared => {
                // Check if we need to run a tick
                let now = Instant::now();
                if now.duration_since(last_tick) >= tick_duration {
                    game_state.tick();
                    last_tick = now;
                }

                // Request redraw to show current state
                window.request_redraw();
                
                // Instead of waiting, we'll poll so the animation remains smooth
                *control_flow = ControlFlow::Poll;
            }
            _ => (),
        }
    });
}

// Create an input stream for various sample formats
fn create_input_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    audio_level: Arc<Mutex<f32>>,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::Sample + Send + 'static,
{
    let err_fn = |err| eprintln!("An error occurred on the audio stream: {}", err);

    device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            // Calculate audio level (root mean square of samples)
            let mut sum_squares = 0.0;
            for &sample in data {
                let sample_f32 = sample.to_f32();
                sum_squares += sample_f32 * sample_f32;
            }
            
            if data.len() > 0 {
                let level = (sum_squares / data.len() as f32).sqrt();
                
                // Amplify the level to make it more visible
                let amplified_level = level * 10.0;
                
                // Update the shared audio level
                if let Ok(mut shared_level) = audio_level.lock() {
                    *shared_level = amplified_level;
                }
            }
        },
        err_fn,
    )
}