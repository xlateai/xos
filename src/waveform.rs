use pixels::{Pixels, SurfaceTexture};
use std::time::{Duration, Instant};
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use crate::audio;

// Increased width for better waveform fidelity
const WIDTH: u32 = 1024;
const HEIGHT: u32 = 256;
const TPS: f32 = 144.0;  // Higher refresh rate for more responsive visualization

// Display constants
const LINE_THICKNESS: usize = 2;
const WAVEFORM_AMPLITUDE: f32 = 0.8; // Scale factor for waveform height (percentage of half-height)

struct AudioVisualizer {
    listener: audio::AudioListener,
    last_tick: Instant,
    waveform_buffer: Vec<f32>,
    // Space for future tensor/matrix integration
    // matrix_data: Option<Matrix>,
}

impl AudioVisualizer {
    fn new() -> Result<Self, String> {
        let device_index = 0;
        let devices = audio::devices();
        let device = devices.get(device_index).unwrap();
        println!("Using device: {}", device.name);

        // Create a new listener with buffer large enough to display full window width
        // This ensures we have enough samples to fill the display
        let buffer_duration = (WIDTH as f32) / 44100.0; // Assuming typical 44.1kHz sample rate
        let listener = match audio::AudioListener::new(&device.device_cpal, buffer_duration) {
            Ok(listener) => listener,
            Err(e) => return Err(format!("Error creating listener: {}", e)),
        };
        
        // Start recording
        if let Err(e) = listener.record() {
            return Err(format!("Failed to start recording: {}", e));
        }
        
        println!("Audio capture started!");
        println!("Sample rate: {} Hz", listener.buffer().sample_rate());
        println!("Channels: {}", listener.buffer().channels());
        
        Ok(Self {
            listener,
            last_tick: Instant::now(),
            waveform_buffer: Vec::with_capacity(WIDTH as usize),
            // matrix_data: None,
        })
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let _delta_time = now.duration_since(self.last_tick).as_secs_f32();
        self.last_tick = now;

        // Get the latest samples from the audio buffer
        let samples = self.listener.buffer().get_samples();
        
        // Update our waveform buffer with the newest samples
        self.waveform_buffer = samples;
        
        // If we have more samples than we can display, trim to fit
        if self.waveform_buffer.len() > WIDTH as usize {
            // Keep most recent samples (right side of the window)
            self.waveform_buffer = self.waveform_buffer
                .iter()
                .skip(self.waveform_buffer.len() - WIDTH as usize)
                .copied()
                .collect();
        }
        
        // If we have fewer samples than our width, pad with zeros
        while self.waveform_buffer.len() < WIDTH as usize {
            self.waveform_buffer.insert(0, 0.0);
        }
    }

    fn draw(&self, frame: &mut [u8]) {
        // Fill the frame with dark background
        for pixel in frame.chunks_exact_mut(4) {
            pixel[0] = 0x10; // R - Dark blue/gray background
            pixel[1] = 0x10; // G
            pixel[2] = 0x18; // B
            pixel[3] = 0xff; // A
        }
        
        // Draw grid lines for reference
        self.draw_grid(frame);
        
        // Draw audio level indicator
        self.draw_level_indicator(frame);
        
        // Draw the waveform
        self.draw_waveform(frame);
        
        // Display audio stats
        self.draw_stats(frame);
    }
    
    fn draw_grid(&self, frame: &mut [u8]) {
        // Draw horizontal center line (zero crossing)
        let center_y = HEIGHT as usize / 2;
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
        
        // Draw vertical grid lines (time divisions)
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
    
    fn draw_waveform(&self, frame: &mut [u8]) {
        let center_y = HEIGHT as usize / 2;
        let half_height = HEIGHT as f32 / 2.0 * WAVEFORM_AMPLITUDE;
        
        // Draw the waveform line connecting points
        for i in 1..self.waveform_buffer.len() {
            let x1 = i - 1;
            let x2 = i;
            
            let sample1 = self.waveform_buffer[x1];
            let sample2 = self.waveform_buffer[x2];
            
            // Calculate y positions (invert because screen coordinates go down)
            let y1 = (center_y as f32 - sample1 * half_height) as isize;
            let y2 = (center_y as f32 - sample2 * half_height) as isize;
            
            // Draw line between points using Bresenham's algorithm
            self.draw_line(frame, x1 as isize, y1, x2 as isize, y2, 0x00, 0xCF, 0xFF);
        }
    }
    
    fn draw_line(&self, frame: &mut [u8], mut x1: isize, mut y1: isize, x2: isize, y2: isize, r: u8, g: u8, b: u8) {
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
    
    fn draw_level_indicator(&self, frame: &mut [u8]) {
        // Get current audio levels
        let rms = self.listener.buffer().get_rms();
        let peak = self.listener.buffer().get_peak();
        
        // Draw level meters at the top
        let meter_y = 20;
        let meter_height = 10;
        let max_width = WIDTH as usize - 20;
        
        // RMS level (green)
        let rms_width = (rms * max_width as f32) as usize;
        for x in 0..rms_width {
            for y in meter_y - meter_height/2..=meter_y + meter_height/2 {
                let index = (y * WIDTH as usize + x + 10) * 4;
                
                // Green gradient
                let intensity = 128 + (x * 127 / max_width);
                frame[index] = 0x00;     // R
                frame[index + 1] = intensity as u8; // G
                frame[index + 2] = 0x00; // B
                frame[index + 3] = 0xff; // A
            }
        }
        
        // Peak level (yellow/red dot)
        let peak_x = (peak.abs() * max_width as f32) as usize;
        if peak_x < max_width {
            for y in (meter_y - meter_height/2 - 2)..=(meter_y + meter_height/2 + 2) {
                for x_offset in -2..=2 {
                    let x = peak_x as isize + x_offset + 10;
                    if x >= 0 && x < WIDTH as isize {
                        let index = (y * WIDTH as usize + x as usize) * 4;
                        
                        // Red for high peaks, yellow for lower
                        if peak.abs() > 0.8 {
                            frame[index] = 0xFF;     // R
                            frame[index + 1] = 0x30; // G
                            frame[index + 2] = 0x30; // B
                        } else {
                            frame[index] = 0xFF;     // R
                            frame[index + 1] = 0xFF; // G
                            frame[index + 2] = 0x00; // B
                        }
                        frame[index + 3] = 0xff; // A
                    }
                }
            }
        }
    }
    
    fn draw_stats(&self, frame: &mut [u8]) {
        // Display sample rate and buffer info at the bottom
        let rms = self.listener.buffer().get_rms();
        let peak = self.listener.buffer().get_peak();
        
        // Draw some stats text indicators as colored blocks
        let stats_y = HEIGHT as usize - 20;
        
        // Draw a level indicator for RMS (0.0-1.0)
        let rms_text = format!("RMS: {:.2}", rms);
        self.draw_text_indicator(frame, 10, stats_y, &rms_text, 0x00, 0xA0, 0x00);
        
        // Draw a level indicator for Peak (0.0-1.0)
        let peak_text = format!("Peak: {:.2}", peak.abs());
        self.draw_text_indicator(frame, 200, stats_y, &peak_text, 0xE0, 0x80, 0x00);
        
        // Draw sample rate
        let rate_text = format!("Rate: {} Hz", self.listener.buffer().sample_rate());
        self.draw_text_indicator(frame, 400, stats_y, &rate_text, 0x80, 0x80, 0xE0);
        
        // Draw buffer size
        let buffer_text = format!("Buffer: {} samples", self.listener.buffer().len());
        self.draw_text_indicator(frame, 600, stats_y, &buffer_text, 0x80, 0xC0, 0xC0);
    }
    
    fn draw_text_indicator(&self, frame: &mut [u8], x: usize, y: usize, text: &str, r: u8, g: u8, b: u8) {
        // Draw a simple colored box to represent text (since we can't actually render text)
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
    
    fn cleanup(&self) {
        // Stop recording when done
        if let Err(e) = self.listener.pause() {
            eprintln!("Error stopping audio: {}", e);
        }
        println!("Audio capture stopped.");
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
        .with_title("XOS High Fidelity Waveform")
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