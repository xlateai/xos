use pixels::{Pixels, SurfaceTexture};
use std::time::{Duration, Instant};
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use cpal::traits::DeviceTrait;

use crate::audio;

const WIDTH: u32 = 512;
const HEIGHT: u32 = 256;
const TPS: f32 = 60.0;
const MIN_RADIUS: f32 = 10.0;
const MAX_RADIUS: f32 = 100.0;
const AUDIO_SENSITIVITY: f32 = 50.0; // Adjust based on your microphone sensitivity

struct AudioVisualizer {
    radius: f32,
    listener: audio::AudioListener,
    last_tick: Instant,
    peak_history: Vec<f32>,
}

impl AudioVisualizer {
    fn new() -> Result<Self, String> {
        // Get device with index 0 (first device)
        let device = match audio::get_device_by_index(0) {
            Some(device) => device,
            None => return Err("Could not find audio device with index 0".to_string()),
        };
        
        // Print device name
        if let Ok(name) = device.name() {
            println!("Listening to device: {}", name);
        }
        
        // Create a new listener with 1 second buffer
        let listener = match audio::AudioListener::new(&device, 1.0) {
            Ok(listener) => listener,
            Err(e) => return Err(format!("Error creating listener: {}", e)),
        };
        
        // Start recording
        if let Err(e) = listener.record() {
            return Err(format!("Failed to start recording: {}", e));
        }
        
        println!("Audio capture started!");
        
        Ok(Self {
            radius: MIN_RADIUS,
            listener,
            last_tick: Instant::now(),
            peak_history: Vec::with_capacity(WIDTH as usize),
        })
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let delta_time = now.duration_since(self.last_tick).as_secs_f32();
        self.last_tick = now;

        // Get the current RMS value from audio buffer
        let rms = self.listener.buffer().get_rms();
        let peak = self.listener.buffer().get_peak();
        
        // Store peak for waveform visualization (store only the most recent WIDTH values)
        self.peak_history.push(peak);
        if self.peak_history.len() > WIDTH as usize {
            self.peak_history.remove(0);
        }
        
        // Map RMS to radius
        let target_radius = MIN_RADIUS + (rms * AUDIO_SENSITIVITY).min(MAX_RADIUS - MIN_RADIUS);
        
        // Smoothly transition to the target radius
        let transition_speed = 5.0; // Units per second
        if self.radius < target_radius {
            self.radius += transition_speed * delta_time;
            if self.radius > target_radius {
                self.radius = target_radius;
            }
        } else {
            self.radius -= transition_speed * delta_time;
            if self.radius < target_radius {
                self.radius = target_radius;
            }
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

        // Draw a green circle in the center that changes with audio level
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
                    
                    // Color based on distance from center (green to yellow)
                    let distance_ratio = (distance_squared.sqrt() / self.radius);
                    frame[index] = (255.0 * distance_ratio) as u8;     // R (more red at edges)
                    frame[index + 1] = 0xff; // G (always green)
                    frame[index + 2] = 0x00; // B
                    frame[index + 3] = 0xff; // A
                }
            }
        }
        
        // Draw waveform at the bottom of the screen
        let waveform_height = 50;
        let base_y = HEIGHT as usize - waveform_height;
        
        for (i, &peak) in self.peak_history.iter().enumerate() {
            if i >= WIDTH as usize {
                break;
            }
            
            // Map peak from [-1,1] to [0,waveform_height]
            let bar_height = (peak.abs() * waveform_height as f32) as usize;
            
            // Draw vertical line for this sample
            for y in 0..bar_height {
                if base_y + y >= HEIGHT as usize {
                    continue;
                }
                
                let index = ((base_y + y) * WIDTH as usize + i) * 4;
                
                // Blue waveform
                frame[index] = 0x00;     // R
                frame[index + 1] = 0x88; // G
                frame[index + 2] = 0xff; // B
                frame[index + 3] = 0xff; // A
            }
        }
        
        // Draw level indicator text showing current RMS
        let rms = self.listener.buffer().get_rms();
        let rms_text_y = 20;
        let rms_level = (rms * 100.0) as usize;
        
        // Draw a simple level bar
        let bar_width = (rms * 100.0) as usize;
        for x in 0..bar_width.min(100) {
            for y in rms_text_y-5..rms_text_y+5 {
                let index = ((y as usize) * WIDTH as usize + x + 10) * 4;
                // Color from green to red based on level
                frame[index] = (255 * x / 100) as u8;     // R
                frame[index + 1] = (255 * (100 - x) / 100) as u8; // G
                frame[index + 2] = 0x00; // B
                frame[index + 3] = 0xff; // A
            }
        }
    }
    
    fn cleanup(&self) {
        // Stop recording when done
        if let Err(e) = self.listener.pause() {
            eprintln!("Error stopping audio: {}", e);
        }
    }
}

pub fn open_waveform() {
    // Create the audio visualizer
    let mut visualizer = match AudioVisualizer::new() {
        Ok(vis) => vis,
        Err(e) => {
            eprintln!("Failed to create audio visualizer: {}", e);
            return;
        }
    };

    // Create the event loop and window
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("XOS Audio Waveform")
        .with_inner_size(LogicalSize::new(WIDTH, HEIGHT))
        .with_resizable(false)
        .build(&event_loop)
        .unwrap();

    // Create the pixel buffer
    let surface_texture = SurfaceTexture::new(WIDTH, HEIGHT, &window);
    let mut pixels = Pixels::new(WIDTH, HEIGHT, surface_texture).unwrap();

    // Timing variables
    let mut last_tick = Instant::now();
    let tick_duration = Duration::from_secs_f32(1.0 / TPS);

    // Run the event loop
    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                visualizer.cleanup();
                *control_flow = ControlFlow::Exit;
            }
            Event::RedrawRequested(_) => {
                // Render the current state
                visualizer.draw(pixels.frame_mut());
                
                if let Err(e) = pixels.render() {
                    eprintln!("pixels.render() failed: {}", e);
                    visualizer.cleanup();
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::MainEventsCleared => {
                // Check if we need to run a tick
                let now = Instant::now();
                if now.duration_since(last_tick) >= tick_duration {
                    visualizer.tick();
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