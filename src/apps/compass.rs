use crate::engine::{Application, EngineState};
use std::f32::consts::PI;

const BACKGROUND_COLOR: (u8, u8, u8) = (32, 32, 32); // dark gray
const TEXT_COLOR: (u8, u8, u8) = (0, 255, 0); // green


pub struct CompassApp;

impl CompassApp {
    pub fn new() -> Self {
        Self
    }

    fn draw_circle(buffer: &mut [u8], w: usize, h: usize, cx: f32, cy: f32, r: f32, col: (u8, u8, u8)) {
        let r2 = r * r;
        for y in (cy as i32 - r as i32)..=(cy as i32 + r as i32) {
            if y < 0 || y >= h as i32 { continue; }
            for x in (cx as i32 - r as i32)..=(cx as i32 + r as i32) {
                if x < 0 || x >= w as i32 { continue; }
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                if dx*dx + dy*dy <= r2 {
                    let i = ((y as usize) * w + (x as usize)) * 4;
                    buffer[i + 0] = col.0;
                    buffer[i + 1] = col.1;
                    buffer[i + 2] = col.2;
                    buffer[i + 3] = 0xff;
                }
            }
        }
    }

    fn draw_text_centered(
        buffer: &mut [u8],
        w: usize,
        h: usize,
        text: &str,
        cx: f32,
        cy: f32,
        scale: f32,
        color: (u8, u8, u8)
    ) {
        // Very crude: each char = filled block pattern.
        // In practice, replace with bitmap font or blitting from texture.
        for (i, ch) in text.chars().enumerate() {
            let offx = cx as i32 + (i as i32 - text.len() as i32 / 2) * (8 * scale as i32 + 2);
            for y in 0..8 {
                for x in 0..8 {
                    if ((x + y + ch as u8) % 7) == 0 {
                        let px = offx + (x as f32 * scale) as i32;
                        let py = cy as i32 + (y as f32 * scale) as i32;
                        if px >= 0 && px < w as i32 && py >= 0 && py < h as i32 {
                            let idx = ((py as usize) * w + (px as usize)) * 4;
                            buffer[idx + 0] = color.0;
                            buffer[idx + 1] = color.1;
                            buffer[idx + 2] = color.2;
                            buffer[idx + 3] = 0xff;
                        }
                    }
                }
            }
        }
    }

    fn draw_compass_overlay(&self, buffer: &mut [u8], w: usize, h: usize, bearing: f32) {
        let cx = w as f32 / 2.0;
        let cy = h as f32 / 2.0;
        let radius = (w.min(h) as f32) * 0.35;

        // Outer circle
        Self::draw_circle(buffer, w, h, cx, cy, radius, (64, 128, 64));

        // Needle (north)
        let angle = (bearing.to_radians() - PI/2.0).rem_euclid(2.0*PI);
        let nx = cx + radius * angle.cos();
        let ny = cy + radius * angle.sin();
        Self::draw_circle(buffer, w, h, nx, ny, 4.0, (0, 255, 0));

        // Labels
        let offset = radius + 20.0;
        let dirs = [("N", 0.0), ("E", PI/2.0), ("S", PI), ("W", 3.0*PI/2.0)];
        for (label, a) in dirs {
            let x = cx + offset * a.cos();
            let y = cy + offset * a.sin();
            Self::draw_text_centered(buffer, w, h, label, x, y, 2.0, TEXT_COLOR);
        }

        // Center bearing text
        let bearing_txt = format!("{:.0}°", bearing);
        Self::draw_text_centered(buffer, w, h, &bearing_txt, cx, cy + radius + 40.0, 1.5, TEXT_COLOR);
    }
}

impl Application for CompassApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let buffer = &mut state.frame.buffer;
        let w = state.frame.width as usize;
        let h = state.frame.height as usize;

        // Clear background
        for i in (0..buffer.len()).step_by(4) {
            buffer[i + 0] = BACKGROUND_COLOR.0;
            buffer[i + 1] = BACKGROUND_COLOR.1;
            buffer[i + 2] = BACKGROUND_COLOR.2;
            buffer[i + 3] = 0xff;
        }

        // Use data if available

        // Draw compass and labels
        self.draw_compass_overlay(buffer, w, h, state.position.bearing as f32);

        // Draw coordinates text
        let coords = format!("{:.4}°, {:.4}°", state.position.latitude, state.position.longitude);
        Self::draw_text_centered(buffer, w, h, &coords, w as f32 / 2.0, h as f32 - 30.0, 1.0, TEXT_COLOR);
    }

    fn on_mouse_down(&mut self, _state: &mut EngineState) {}
    fn on_mouse_up(&mut self, _state: &mut EngineState) {}
    fn on_mouse_move(&mut self, _state: &mut EngineState) {}
}
