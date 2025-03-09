use pixels::{Pixels, SurfaceTexture};
use std::time::{Duration, Instant};
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use crate::audio;

// Display constants
const WIDTH: u32 = 1024;
const HEIGHT: u32 = 256;
const TPS: f32 = 144.0;  // Frames per second
const LINE_THICKNESS: usize = 2;
const WAVEFORM_AMPLITUDE: f32 = 0.8; // Scale factor for waveform height

struct AudioVisualizer {
    listener_handle: std::sync::Arc<std::sync::Mutex<audio::AudioListener>>,
    last_tick: Instant,
    channel_waveforms: Vec<Vec<f32>>,
}

impl AudioVisualizer {
    fn new() -> Result<Self, String> {
        // Get the global audio listener
        let listener_handle = audio::get_listener()?;
        
        // Initialize empty waveform buffers for channels
        // We'll populate this in the first tick
        let channel_waveforms = Vec::new();
        
        Ok(Self {
            listener_handle,
            last_tick: Instant::now(),
            channel_waveforms,
        })
    }

    fn tick(&mut self) {
        let now = Instant::now();
        self.last_tick = now;

        // Get all samples from all devices
        if let Ok(all_device_samples) = audio::get_all_samples() {
            // Clear previous waveform data
            self.channel_waveforms.clear();
            
            // Process each device's samples
            for (_, (device_samples, _)) in all_device_samples {
                for channel_samples in device_samples {
                    // Resize samples to fit our display width
                    let resized_samples = resize_samples(&channel_samples, WIDTH as usize);
                    self.channel_waveforms.push(resized_samples);
                }
            }
        }
    }

    fn draw(&self, frame: &mut [u8]) {
        // Clear frame with dark background
        for pixel in frame.chunks_exact_mut(4) {
            pixel[0] = 0x10; // R
            pixel[1] = 0x10; // G
            pixel[2] = 0x18; // B
            pixel[3] = 0xff; // A
        }
        
        // Draw grid
        self.draw_grid(frame);
        
        // Draw each channel's waveform
        self.draw_waveforms(frame);
        
        // Draw status text
        self.draw_status(frame);
    }
    
    fn draw_grid(&self, frame: &mut [u8]) {
        let center_y = HEIGHT as usize / 2;
        
        // Draw horizontal center line
        for x in 0..WIDTH as usize {
            for y_offset in 0..LINE_THICKNESS {
                let y = center_y + y_offset - (LINE_THICKNESS / 2);
                if y < HEIGHT as usize {
                    let index = (y * WIDTH as usize + x) * 4;
                    frame[index] = 0x40;     // R
                    frame[index + 1] = 0x40; // G
                    frame[index + 2] = 0x40; // B
                    frame[index + 3] = 0xff; // A
                }
            }
        }
        
        // Draw vertical time divisions
        for grid_x in (0..WIDTH as usize).step_by(WIDTH as usize / 8) {
            for x_offset in 0..LINE_THICKNESS {
                let x = grid_x + x_offset;
                if x < WIDTH as usize {
                    for y in 0..HEIGHT as usize {
                        let index = (y * WIDTH as usize + x) * 4;
                        frame[index] = 0x20;     // R
                        frame[index + 1] = 0x20; // G
                        frame[index + 2] = 0x20; // B
                        frame[index + 3] = 0xff; // A
                    }
                }
            }
        }
        
        // Draw horizontal amplitude grid lines
        for amplitude in [0.25, 0.5, 0.75] {
            for direction in [-1, 1] {
                let y = (center_y as f32 + direction as f32 * amplitude * center_y as f32) as usize;
                if y < HEIGHT as usize {
                    for x in 0..WIDTH as usize {
                        let index = (y * WIDTH as usize + x) * 4;
                        frame[index] = 0x20;     // R
                        frame[index + 1] = 0x20; // G
                        frame[index + 2] = 0x20; // B
                        frame[index + 3] = 0xff; // A
                    }
                }
            }
        }
    }
    
    fn draw_waveforms(&self, frame: &mut [u8]) {
        if self.channel_waveforms.is_empty() {
            return;
        }
        
        let center_y = HEIGHT as usize / 2;
        let half_height = HEIGHT as f32 / 2.0 * WAVEFORM_AMPLITUDE;
        
        // Try to get device colors from the listener
        let device_colors = if let Ok(device_names) = audio::get_device_names() {
            if let Ok(listener) = self.listener_handle.lock() {
                device_names.iter()
                    .filter_map(|name| listener.get_device_color(name))
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        
        // Draw each channel's waveform
        for (channel_idx, waveform) in self.channel_waveforms.iter().enumerate() {
            if waveform.is_empty() {
                continue;
            }
            
            // Get color for this channel - cycle through device colors
            let color = if !device_colors.is_empty() {
                device_colors[channel_idx % device_colors.len()]
            } else {
                // Fallback colors if we couldn't get device colors
                match channel_idx % 6 {
                    0 => (0, 255, 0),   // Green
                    1 => (255, 0, 0),   // Red
                    2 => (0, 150, 255), // Blue
                    3 => (255, 255, 0), // Yellow
                    4 => (255, 0, 255), // Magenta
                    _ => (0, 255, 255), // Cyan
                }
            };
            
            // Draw the waveform line
            for i in 1..waveform.len() {
                let x1 = i - 1;
                let x2 = i;
                
                let sample1 = waveform[x1];
                let sample2 = waveform[x2];
                
                // Calculate y positions (invert because screen coordinates go down)
                let y1 = (center_y as f32 - sample1 * half_height) as isize;
                let y2 = (center_y as f32 - sample2 * half_height) as isize;
                
                // Draw line between points
                draw_line(frame, 
                    x1 as isize, y1, 
                    x2 as isize, y2, 
                    color.0, color.1, color.2);
            }
        }
    }
    
    fn draw_status(&self, frame: &mut [u8]) {
        // Get number of devices and channels
        let (num_devices, num_channels) = if let Ok(listener) = self.listener_handle.lock() {
            (listener.device_count(), self.channel_waveforms.len())
        } else {
            (0, 0)
        };
        
        // Draw text indicators
        draw_text_indicator(frame, 10, HEIGHT as usize - 20, 
            &format!("Devices: {}", num_devices), 
            0x80, 0xC0, 0xC0);
            
        draw_text_indicator(frame, 200, HEIGHT as usize - 20, 
            &format!("Channels: {}", num_channels), 
            0x80, 0xC0, 0xFF);
    }
}

// Helper function to resize samples to fit display width
fn resize_samples(samples: &[f32], target_width: usize) -> Vec<f32> {
    if samples.is_empty() {
        return vec![0.0; target_width];
    }
    
    if samples.len() == target_width {
        return samples.to_vec();
    }
    
    let mut result = Vec::with_capacity(target_width);
    
    if samples.len() > target_width {
        // Downsample - take max value in each bucket
        let step = samples.len() as f32 / target_width as f32;
        
        for i in 0..target_width {
            let start = (i as f32 * step) as usize;
            let end = ((i + 1) as f32 * step).min(samples.len() as f32) as usize;
            
            if start < end {
                let mut max_val = samples[start].abs();
                for j in start+1..end {
                    let abs_val = samples[j].abs();
                    if abs_val > max_val {
                        max_val = abs_val;
                    }
                }
                result.push(if max_val > 0.0 { 
                    max_val * samples[start].signum() 
                } else { 
                    0.0 
                });
            } else if start < samples.len() {
                result.push(samples[start]);
            } else {
                result.push(0.0);
            }
        }
    } else {
        // Upsample - linear interpolation
        let step = (samples.len() - 1) as f32 / (target_width - 1) as f32;
        
        for i in 0..target_width {
            let pos = i as f32 * step;
            let index = pos as usize;
            let frac = pos - index as f32;
            
            if index + 1 < samples.len() {
                let val = samples[index] * (1.0 - frac) + samples[index + 1] * frac;
                result.push(val);
            } else if index < samples.len() {
                result.push(samples[index]);
            } else {
                result.push(0.0);
            }
        }
    }
    
    result
}

// Helper function to draw a line
fn draw_line(frame: &mut [u8], mut x1: isize, mut y1: isize, x2: isize, y2: isize, r: u8, g: u8, b: u8) {
    // Bresenham's line algorithm
    let dx = (x2 - x1).abs();
    let dy = -(y2 - y1).abs();
    let sx = if x1 < x2 { 1 } else { -1 };
    let sy = if y1 < y2 { 1 } else { -1 };
    let mut err = dx + dy;
    
    loop {
        // Draw a thicker line point
        let offset = (LINE_THICKNESS as isize) / 2;
        for y_offset in -offset..=offset {
            for x_offset in -offset..=offset {
                let x = x1 + x_offset;
                let y = y1 + y_offset;
                
                if x >= 0 && x < WIDTH as isize && y >= 0 && y < HEIGHT as isize {
                    let index = (y as usize * WIDTH as usize + x as usize) * 4;
                    frame[index] = r;
                    frame[index + 1] = g;
                    frame[index + 2] = b;
                    frame[index + 3] = 0xff;
                }
            }
        }
        
        if x1 == x2 && y1 == y2 {
            break;
        }
        
        let e2 = 2 * err;
        if e2 >= dy {
            if x1 == x2 {
                break;
            }
            err += dy;
            x1 += sx;
        }
        if e2 <= dx {
            if y1 == y2 {
                break;
            }
            err += dx;
            y1 += sy;
        }
    }
}

// Helper function to draw text indicator
fn draw_text_indicator(frame: &mut [u8], x: usize, y: usize, text: &str, r: u8, g: u8, b: u8) {
    // Draw a simple colored box to represent text
    let width = text.len() * 8; // Approximate width based on text length
    let height = 16;
    
    // Draw the box
    for dy in 0..height {
        for dx in 0..width {
            if x + dx < WIDTH as usize && y + dy < HEIGHT as usize {
                let index = ((y + dy) * WIDTH as usize + (x + dx)) * 4;
                
                // Make the inside slightly darker
                let inner_r = (r as u16 * 2/3) as u8;
                let inner_g = (g as u16 * 2/3) as u8;
                let inner_b = (b as u16 * 2/3) as u8;
                
                // Draw border
                if dx == 0 || dy == 0 || dx == width-1 || dy == height-1 {
                    frame[index] = r;
                    frame[index + 1] = g;
                    frame[index + 2] = b;
                } else {
                    frame[index] = inner_r;
                    frame[index + 1] = inner_g;
                    frame[index + 2] = inner_b;
                }
                frame[index + 3] = 0xff; // A
            }
        }
    }
}

/// Opens a window with a real-time audio waveform display for all detected audio devices.
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
        .with_title("Audio Waveform Visualizer")
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
                *control_flow = ControlFlow::Exit;
            }
            Event::RedrawRequested(_) => {
                // Render the current state
                visualizer.draw(pixels.frame_mut());
                
                if let Err(e) = pixels.render() {
                    eprintln!("pixels.render() failed: {}", e);
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