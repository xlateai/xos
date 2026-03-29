//! Floating overlay demo: small always-on-top square (see [`crate::engine::start_overlay_native`]).

use crate::engine::{Application, EngineState};
use crate::rasterizer::{fill, fill_rect};

const BAR_HEIGHT: u32 = 36;
const BG: (u8, u8, u8, u8) = (12, 14, 22, 255);
const BAR: (u8, u8, u8, u8) = (40, 44, 58, 255);

pub struct OverlayApp;

impl OverlayApp {
    pub fn new() -> Self {
        Self
    }
}

impl Application for OverlayApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let w = state.frame.shape()[1] as u32;
        let h = state.frame.shape()[0] as u32;
        fill(&mut state.frame, BG);
        if w > 0 && h > 0 {
            let bar_h = BAR_HEIGHT.min(h);
            fill_rect(
                &mut state.frame,
                0,
                0,
                w as i32,
                bar_h as i32,
                BAR,
            );
        }
    }
}
