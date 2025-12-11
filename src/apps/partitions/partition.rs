use crate::engine::EngineState;
use crate::tuneable::write_all_to_source;

#[derive(PartialEq, Clone, Copy)]
pub enum DragRegion {
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

pub struct Partition {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
    pub color: (u8, u8, u8),
    pub dragging: bool,
    pub drag_region: DragRegion,
    pub drag_offset_x: f32,
    pub drag_offset_y: f32,
}

impl Partition {
    pub fn new(left: f32, right: f32, top: f32, bottom: f32, color: (u8, u8, u8)) -> Self {
        Self {
            left,
            right,
            top,
            bottom,
            color,
            dragging: false,
            drag_region: DragRegion::None,
            drag_offset_x: 0.0,
            drag_offset_y: 0.0,
        }
    }

    pub fn draw(&self, buffer: &mut [u8], width: u32, height: u32) -> (i32, i32, u32, u32) {
        let w = width as f32;
        let h = height as f32;

        let x0 = (self.left * w).round().clamp(0.0, w) as i32;
        let x1 = (self.right * w).round().clamp(0.0, w) as i32;
        let y0 = (self.top * h).round().clamp(0.0, h) as i32;
        let y1 = (self.bottom * h).round().clamp(0.0, h) as i32;

        let rect_w = (x1 - x0).max(0) as u32;
        let rect_h = (y1 - y0).max(0) as u32;

        for dy in 0..rect_h {
            for dx in 0..rect_w {
                let sx = x0 + dx as i32;
                let sy = y0 + dy as i32;

                if sx >= 0 && sy >= 0 && (sx as u32) < width && (sy as u32) < height {
                    let idx = ((sy as u32 * width + sx as u32) * 4) as usize;
                    buffer[idx + 0] = self.color.0;
                    buffer[idx + 1] = self.color.1;
                    buffer[idx + 2] = self.color.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }

        (x0, y0, rect_w, rect_h)
    }

    pub fn region_under_mouse(&self, mx: f32, my: f32, w: f32, h: f32) -> DragRegion {
        let left = self.left * w;
        let right = self.right * w;
        let top = self.top * h;
        let bottom = self.bottom * h;
        let near = 8.0;

        // Check if mouse is within the bounds of the partition (with some tolerance)
        let within_horizontal = mx >= (left - near) && mx <= (right + near);
        let within_vertical = my >= (top - near) && my <= (bottom + near);
        
        // Check proximity to edges, but only if mouse is within the partition bounds
        let near_left = (mx - left).abs() <= near && my >= (top - near) && my <= (bottom + near);
        let near_right = (mx - right).abs() <= near && my >= (top - near) && my <= (bottom + near);
        let near_top = (my - top).abs() <= near && mx >= (left - near) && mx <= (right + near);
        let near_bottom = (my - bottom).abs() <= near && mx >= (left - near) && mx <= (right + near);

        // Check corners (must be near both edges)
        let near_top_left = (mx - left).abs() <= near && (my - top).abs() <= near;
        let near_top_right = (mx - right).abs() <= near && (my - top).abs() <= near;
        let near_bottom_left = (mx - left).abs() <= near && (my - bottom).abs() <= near;
        let near_bottom_right = (mx - right).abs() <= near && (my - bottom).abs() <= near;

        match (near_top_left, near_top_right, near_bottom_left, near_bottom_right, near_left, near_right, near_top, near_bottom) {
            (true, false, false, false, _, _, _, _) => DragRegion::TopLeft,
            (false, true, false, false, _, _, _, _) => DragRegion::TopRight,
            (false, false, true, false, _, _, _, _) => DragRegion::BottomLeft,
            (false, false, false, true, _, _, _, _) => DragRegion::BottomRight,
            (false, false, false, false, true, false, false, false) => DragRegion::Left,
            (false, false, false, false, false, true, false, false) => DragRegion::Right,
            (false, false, false, false, false, false, true, false) => DragRegion::Top,
            (false, false, false, false, false, false, false, true) => DragRegion::Bottom,
            _ if mx > left && mx < right && my > top && my < bottom => DragRegion::Center,
            _ => DragRegion::None,
        }
    }

    pub fn on_mouse_move(
        &mut self,
        mx: f32,
        my: f32,
        w: f32,
        h: f32,
        set_left: impl Fn(f32),
        set_right: impl Fn(f32),
        set_top: impl Fn(f32),
        set_bottom: impl Fn(f32),
    ) {
        if self.dragging {
            match self.drag_region {
                DragRegion::Left => {
                    let new_left = (mx / w).min(self.right - 0.01);
                    set_left(new_left.clamp(0.0, 1.0));
                    self.left = new_left.clamp(0.0, 1.0);
                }
                DragRegion::Right => {
                    let new_right = (mx / w).max(self.left + 0.01);
                    set_right(new_right.clamp(0.0, 1.0));
                    self.right = new_right.clamp(0.0, 1.0);
                }
                DragRegion::Top => {
                    let new_top = (my / h).min(self.bottom - 0.01);
                    set_top(new_top.clamp(0.0, 1.0));
                    self.top = new_top.clamp(0.0, 1.0);
                }
                DragRegion::Bottom => {
                    let new_bottom = (my / h).max(self.top + 0.01);
                    set_bottom(new_bottom.clamp(0.0, 1.0));
                    self.bottom = new_bottom.clamp(0.0, 1.0);
                }
                DragRegion::TopLeft => {
                    let new_left = (mx / w).min(self.right - 0.01);
                    let new_top = (my / h).min(self.bottom - 0.01);
                    set_left(new_left.clamp(0.0, 1.0));
                    set_top(new_top.clamp(0.0, 1.0));
                    self.left = new_left.clamp(0.0, 1.0);
                    self.top = new_top.clamp(0.0, 1.0);
                }
                DragRegion::TopRight => {
                    let new_right = (mx / w).max(self.left + 0.01);
                    let new_top = (my / h).min(self.bottom - 0.01);
                    set_right(new_right.clamp(0.0, 1.0));
                    set_top(new_top.clamp(0.0, 1.0));
                    self.right = new_right.clamp(0.0, 1.0);
                    self.top = new_top.clamp(0.0, 1.0);
                }
                DragRegion::BottomLeft => {
                    let new_left = (mx / w).min(self.right - 0.01);
                    let new_bottom = (my / h).max(self.top + 0.01);
                    set_left(new_left.clamp(0.0, 1.0));
                    set_bottom(new_bottom.clamp(0.0, 1.0));
                    self.left = new_left.clamp(0.0, 1.0);
                    self.bottom = new_bottom.clamp(0.0, 1.0);
                }
                DragRegion::BottomRight => {
                    let new_right = (mx / w).max(self.left + 0.01);
                    let new_bottom = (my / h).max(self.top + 0.01);
                    set_right(new_right.clamp(0.0, 1.0));
                    set_bottom(new_bottom.clamp(0.0, 1.0));
                    self.right = new_right.clamp(0.0, 1.0);
                    self.bottom = new_bottom.clamp(0.0, 1.0);
                }
                DragRegion::Center => {
                    let dx = (mx - self.drag_offset_x) / w;
                    let dy = (my - self.drag_offset_y) / h;

                    let width = self.right - self.left;
                    let height = self.bottom - self.top;

                    let new_left = (self.left + dx).clamp(0.0, 1.0 - width);
                    let new_top = (self.top + dy).clamp(0.0, 1.0 - height);

                    set_left(new_left);
                    set_right(new_left + width);
                    set_top(new_top);
                    set_bottom(new_top + height);

                    self.left = new_left;
                    self.right = new_left + width;
                    self.top = new_top;
                    self.bottom = new_top + height;

                    // update drag offset for smooth continuous motion
                    self.drag_offset_x = mx;
                    self.drag_offset_y = my;
                }
                _ => {}
            }

            write_all_to_source();
        }
    }

    pub fn on_mouse_down(&mut self, mx: f32, my: f32, w: f32, h: f32) {
        let region = self.region_under_mouse(mx, my, w, h);
        if region != DragRegion::None {
            self.dragging = true;
            self.drag_region = region;
            self.drag_offset_x = mx;
            self.drag_offset_y = my;
        }
    }

    pub fn on_mouse_up(&mut self) {
        self.dragging = false;
        self.drag_region = DragRegion::None;
    }

    pub fn update_cursor_style(&self, state: &mut EngineState, region: DragRegion) {
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
    }
}
