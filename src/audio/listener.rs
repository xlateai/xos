use cpal::traits::HostTrait;
use std::collections::HashMap;

use super::device::AudioDevice;

pub struct AudioListener {
    devices: Vec<AudioDevice>,
    device_colors: HashMap<String, (u8, u8, u8)>, // RGB colors for each device
}

impl AudioListener {
    pub(crate) fn new() -> Result<Self, String> {
        let mut listener = Self {
            devices: Vec::new(),
            device_colors: HashMap::new(),
        };
        
        listener.scan_devices()?;
        listener.assign_device_colors();
        
        Ok(listener)
    }

    fn scan_devices(&mut self) -> Result<(), String> {
        self.devices.clear();
        
        let host = cpal::default_host();
        
        // Get input devices
        let input_devices = host.input_devices()
            .map_err(|e| format!("Error getting input devices: {}", e))?;
            
        for device in input_devices {
            match AudioDevice::new(device, 44100) { // 1-second buffer at 44.1kHz
                Ok(audio_device) => self.devices.push(audio_device),
                Err(e) => eprintln!("Error creating audio device: {}", e),
            }
        }
        
        // Get output devices
        let output_devices = host.output_devices()
            .map_err(|e| format!("Error getting output devices: {}", e))?;
            
        for device in output_devices {
            match AudioDevice::new(device, 44100) {
                Ok(audio_device) => self.devices.push(audio_device),
                Err(e) => eprintln!("Error creating audio device: {}", e),
            }
        }
        
        if self.devices.is_empty() {
            return Err("No audio devices found".to_string());
        }
        
        Ok(())
    }

    fn assign_device_colors(&mut self) {
        // Assign a unique color to each device for visualization
        let mut hue = 0.0;
        let hue_step = 360.0 / self.devices.len() as f32;
        
        for device in &self.devices {
            let (r, g, b) = hue_to_rgb(hue, 0.8, 0.7);
            self.device_colors.insert(device.name().to_string(), (r, g, b));
            hue += hue_step;
        }
    }

    pub fn start_listening(&mut self) -> Result<(), String> {
        for device in &mut self.devices {
            if let Err(e) = device.start() {
                eprintln!("Error starting device {}: {}", device.name(), e);
            }
        }
        
        Ok(())
    }

    pub fn stop_listening(&mut self) {
        for device in &mut self.devices {
            device.stop();
        }
    }

    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    pub fn get_device_names(&self) -> Vec<String> {
        self.devices.iter().map(|d| d.name().to_string()).collect()
    }

    pub fn get_device_color(&self, name: &str) -> Option<(u8, u8, u8)> {
        self.device_colors.get(name).copied()
    }

    pub fn get_samples(&self) -> HashMap<String, (Vec<Vec<f32>>, (u8, u8, u8))> {
        let mut result = HashMap::new();
        
        for device in &self.devices {
            let name = device.name().to_string();
            let samples = device.get_samples();
            let color = self.get_device_color(&name).unwrap_or((255, 255, 255));
            
            result.insert(name, (samples, color));
        }
        
        result
    }
}

// Helper function to convert HSV to RGB
fn hue_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    
    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    
    let r = ((r + m) * 255.0) as u8;
    let g = ((g + m) * 255.0) as u8;
    let b = ((b + m) * 255.0) as u8;
    
    (r, g, b)
}