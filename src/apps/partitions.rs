use crate::engine::{Application, EngineState};
use crate::tuneable::write_all_to_source;
use crate::tuneables;

// Rectangle 0
tuneables! {
    rect0_left: f32 = 0.1;
    rect0_right: f32 = 0.4;
    rect0_top: f32 = 0.15;
    rect0_bottom: f32 = 0.5;
}

// Rectangle 1
tuneables! {
    rect1_left: f32 = 0.55;
    rect1_right: f32 = 0.85;
    rect1_top: f32 = 0.2;
    rect1_bottom: f32 = 0.6;
}

// Rectangle 2
tuneables! {
    rect2_left: f32 = 0.2;
    rect2_right: f32 = 0.7;
    rect2_top: f32 = 0.65;
    rect2_bottom: f32 = 0.9;
}

const BACKGROUND_COLOR: (u8, u8, u8) = (32, 32, 32);
const RECT_COLOR_0: (u8, u8, u8) = (100, 150, 255);
const RECT_COLOR_1: (u8, u8, u8) = (255, 150, 100);
const RECT_COLOR_2: (u8, u8, u8) = (150, 255, 100);

#[derive(PartialEq, Clone, Copy)]
enum DragRegion {
    None,
    Center,
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

struct Rectangle {
    left: fn() -> &'static crate::tuneable::Tuneable<f32>,
    right: fn() -> &'static crate::tuneable::Tuneable<f32>,
    top: fn() -> &'static crate::tuneable::Tuneable<f32>,
    bottom: fn() -> &'static crate::tuneable::Tuneable<f32>,
    color: (u8, u8, u8),
}

pub struct Partitions {
    rectangles: Vec<Rectangle>,
    dragging_rect: Option<usize>,
    dragging_region: DragRegion,
    drag_offset_x: f32,
    drag_offset_y: f32,
}

impl Partitions {
    pub fn new() -> Self {
        Self {
            rectangles: vec![
                Rectangle {
                    left: rect0_left,
                    right: rect0_right,
                    top: rect0_top,
                    bottom: rect0_bottom,
                    color: RECT_COLOR_0,
                },
                Rectangle {
                    left: rect1_left,
                    right: rect1_right,
                    top: rect1_top,
                    bottom: rect1_bottom,
                    color: RECT_COLOR_1,
                },
                Rectangle {
                    left: rect2_left,
                    right: rect2_right,
                    top: rect2_top,
                    bottom: rect2_bottom,
                    color: RECT_COLOR_2,
                },
            ],
            dragging_rect: None,
            dragging_region: DragRegion::None,
            drag_offset_x: 0.0,
            drag_offset_y: 0.0,
        }
    }

    fn draw_rectangle(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        rect: &Rectangle,
    ) -> (i32, i32, u32, u32) {
        let w = width as f32;
        let h = height as f32;

        let x0 = ((rect.left)().get() * w).round().clamp(0.0, w) as i32;
        let x1 = ((rect.right)().get() * w).round().clamp(0.0, w) as i32;
        let y0 = ((rect.top)().get() * h).round().clamp(0.0, h) as i32;
        let y1 = ((rect.bottom)().get() * h).round().clamp(0.0, h) as i32;

        let rect_w = (x1 - x0).max(0) as u32;
        let rect_h = (y1 - y0).max(0) as u32;

        for dy in 0..rect_h {
            for dx in 0..rect_w {
                let sx = x0 + dx as i32;
                let sy = y0 + dy as i32;

                if sx >= 0 && sy >= 0 && (sx as u32) < width && (sy as u32) < height {
                    let idx = ((sy as u32 * width + sx as u32) * 4) as usize;
                    buffer[idx + 0] = rect.color.0;
                    buffer[idx + 1] = rect.color.1;
                    buffer[idx + 2] = rect.color.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }

        (x0, y0, rect_w, rect_h)
    }

    fn region_under_mouse(
        &self,
        mx: f32,
        my: f32,
        w: f32,
        h: f32,
        rect: &Rectangle,
    ) -> DragRegion {
        let left = (rect.left)().get() * w;
        let right = (rect.right)().get() * w;
        let top = (rect.top)().get() * h;
        let bottom = (rect.bottom)().get() * h;
        let near = 8.0;

        let near_left = (mx - left).abs() <= near;
        let near_right = (mx - right).abs() <= near;
        let near_top = (my - top).abs() <= near;
        let near_bottom = (my - bottom).abs() <= near;

        match (near_left, near_right, near_top, near_bottom) {
            (true, false, true, false) => DragRegion::TopLeft,
            (false, true, true, false) => DragRegion::TopRight,
            (true, false, false, true) => DragRegion::BottomLeft,
            (false, true, false, true) => DragRegion::BottomRight,
            (true, false, false, false) => DragRegion::Left,
            (false, true, false, false) => DragRegion::Right,
            (false, false, true, false) => DragRegion::Top,
            (false, false, false, true) => DragRegion::Bottom,
            _ if mx > left && mx < right && my > top && my < bottom => DragRegion::Center,
            _ => DragRegion::None,
        }
    }
}

impl Application for Partitions {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        state.frame.buffer.chunks_exact_mut(4).for_each(|p| {
            p[0] = BACKGROUND_COLOR.0;
            p[1] = BACKGROUND_COLOR.1;
            p[2] = BACKGROUND_COLOR.2;
            p[3] = 0xff;
        });

        for rect in &self.rectangles {
            self.draw_rectangle(
                &mut state.frame.buffer,
                state.frame.width,
                state.frame.height,
                rect,
            );
        }
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        let mx = state.mouse.x;
        let my = state.mouse.y;
        let w = state.frame.width as f32;
        let h = state.frame.height as f32;

        if let Some(rect_idx) = self.dragging_rect {
            let rect = &self.rectangles[rect_idx];
            match self.dragging_region {
                DragRegion::Left => {
                    let new_left = (mx / w).min((rect.right)().get() - 0.01);
                    (rect.left)().set(new_left.clamp(0.0, 1.0));
                }
                DragRegion::Right => {
                    let new_right = (mx / w).max((rect.left)().get() + 0.01);
                    (rect.right)().set(new_right.clamp(0.0, 1.0));
                }
                DragRegion::Top => {
                    let new_top = (my / h).min((rect.bottom)().get() - 0.01);
                    (rect.top)().set(new_top.clamp(0.0, 1.0));
                }
                DragRegion::Bottom => {
                    let new_bottom = (my / h).max((rect.top)().get() + 0.01);
                    (rect.bottom)().set(new_bottom.clamp(0.0, 1.0));
                }
                DragRegion::TopLeft => {
                    let new_left = (mx / w).min((rect.right)().get() - 0.01);
                    let new_top = (my / h).min((rect.bottom)().get() - 0.01);
                    (rect.left)().set(new_left.clamp(0.0, 1.0));
                    (rect.top)().set(new_top.clamp(0.0, 1.0));
                }
                DragRegion::TopRight => {
                    let new_right = (mx / w).max((rect.left)().get() + 0.01);
                    let new_top = (my / h).min((rect.bottom)().get() - 0.01);
                    (rect.right)().set(new_right.clamp(0.0, 1.0));
                    (rect.top)().set(new_top.clamp(0.0, 1.0));
                }
                DragRegion::BottomLeft => {
                    let new_left = (mx / w).min((rect.right)().get() - 0.01);
                    let new_bottom = (my / h).max((rect.top)().get() + 0.01);
                    (rect.left)().set(new_left.clamp(0.0, 1.0));
                    (rect.bottom)().set(new_bottom.clamp(0.0, 1.0));
                }
                DragRegion::BottomRight => {
                    let new_right = (mx / w).max((rect.left)().get() + 0.01);
                    let new_bottom = (my / h).max((rect.top)().get() + 0.01);
                    (rect.right)().set(new_right.clamp(0.0, 1.0));
                    (rect.bottom)().set(new_bottom.clamp(0.0, 1.0));
                }
                DragRegion::Center => {
                    let dx = (mx - self.drag_offset_x) / w;
                    let dy = (my - self.drag_offset_y) / h;

                    let width = (rect.right)().get() - (rect.left)().get();
                    let height = (rect.bottom)().get() - (rect.top)().get();

                    let new_left = ((rect.left)().get() + dx).clamp(0.0, 1.0 - width);
                    let new_top = ((rect.top)().get() + dy).clamp(0.0, 1.0 - height);

                    (rect.left)().set(new_left);
                    (rect.right)().set(new_left + width);
                    (rect.top)().set(new_top);
                    (rect.bottom)().set(new_top + height);

                    // update drag offset for smooth continuous motion
                    self.drag_offset_x = mx;
                    self.drag_offset_y = my;
                }
                _ => {}
            }

            write_all_to_source();
        } else {
            // Check which rectangle is under the mouse and set cursor style
            let mut found = false;
            for rect in &self.rectangles {
                let region = self.region_under_mouse(mx, my, w, h, rect);
                if region != DragRegion::None {
                    match region {
                        DragRegion::Left | DragRegion::Right => {
                            state.mouse.style.resize_horizontal();
                        }
                        DragRegion::Top | DragRegion::Bottom => {
                            state.mouse.style.resize_vertical();
                        }
                        DragRegion::TopLeft | DragRegion::BottomRight => {
                            state.mouse.style.resize_diagonal_nw();
                        }
                        DragRegion::TopRight | DragRegion::BottomLeft => {
                            state.mouse.style.resize_diagonal_ne();
                        }
                        _ => state.mouse.style.default(),
                    }
                    found = true;
                    break;
                }
            }
            if !found {
                state.mouse.style.default();
            }
        }
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        let w = state.frame.width as f32;
        let h = state.frame.height as f32;
        let mx = state.mouse.x;
        let my = state.mouse.y;

        // Find which rectangle (if any) is under the mouse
        for (idx, rect) in self.rectangles.iter().enumerate() {
            let region = self.region_under_mouse(mx, my, w, h, rect);
            if region != DragRegion::None {
                self.dragging_rect = Some(idx);
                self.dragging_region = region;
                self.drag_offset_x = mx;
                self.drag_offset_y = my;
                break;
            }
        }
    }

    fn on_mouse_up(&mut self, _state: &mut EngineState) {
        self.dragging_rect = None;
        self.dragging_region = DragRegion::None;
    }
}
