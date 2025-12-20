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

pub struct PartitionData {
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

impl PartitionData {
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
}

pub trait PartitionSetters {
    fn set_left(&self, v: f32);
    fn set_right(&self, v: f32);
    fn set_top(&self, v: f32);
    fn set_bottom(&self, v: f32);
}

pub trait Partition {
    fn data(&self) -> &PartitionData;
    fn data_mut(&mut self) -> &mut PartitionData;

    fn left(&self) -> f32 {
        self.data().left
    }

    fn right(&self) -> f32 {
        self.data().right
    }

    fn top(&self) -> f32 {
        self.data().top
    }

    fn bottom(&self) -> f32 {
        self.data().bottom
    }

    fn set_left(&mut self, v: f32) {
        self.data_mut().left = v;
    }

    fn set_right(&mut self, v: f32) {
        self.data_mut().right = v;
    }

    fn set_top(&mut self, v: f32) {
        self.data_mut().top = v;
    }

    fn set_bottom(&mut self, v: f32) {
        self.data_mut().bottom = v;
    }

    fn dragging(&self) -> bool {
        self.data().dragging
    }

    fn draw(&self, buffer: &mut [u8], width: u32, height: u32) {
        // Blank - partitions are invisible by default unless taken ownership over by another module
        let _ = (buffer, width, height);
    }

    fn region_under_mouse(&self, mx: f32, my: f32, w: f32, h: f32) -> DragRegion {
        let data = self.data();
        let left = data.left * w;
        let right = data.right * w;
        let top = data.top * h;
        let bottom = data.bottom * h;
        let near = 8.0;

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

    fn on_mouse_move(&mut self, mx: f32, my: f32, w: f32, h: f32, setters: &dyn PartitionSetters) {
        let data = self.data_mut();
        if data.dragging {
            match data.drag_region {
                DragRegion::Left => {
                    let new_left = (mx / w).min(data.right - 0.01);
                    setters.set_left(new_left.clamp(0.0, 1.0));
                    data.left = new_left.clamp(0.0, 1.0);
                }
                DragRegion::Right => {
                    let new_right = (mx / w).max(data.left + 0.01);
                    setters.set_right(new_right.clamp(0.0, 1.0));
                    data.right = new_right.clamp(0.0, 1.0);
                }
                DragRegion::Top => {
                    let new_top = (my / h).min(data.bottom - 0.01);
                    setters.set_top(new_top.clamp(0.0, 1.0));
                    data.top = new_top.clamp(0.0, 1.0);
                }
                DragRegion::Bottom => {
                    let new_bottom = (my / h).max(data.top + 0.01);
                    setters.set_bottom(new_bottom.clamp(0.0, 1.0));
                    data.bottom = new_bottom.clamp(0.0, 1.0);
                }
                DragRegion::TopLeft => {
                    let new_left = (mx / w).min(data.right - 0.01);
                    let new_top = (my / h).min(data.bottom - 0.01);
                    setters.set_left(new_left.clamp(0.0, 1.0));
                    setters.set_top(new_top.clamp(0.0, 1.0));
                    data.left = new_left.clamp(0.0, 1.0);
                    data.top = new_top.clamp(0.0, 1.0);
                }
                DragRegion::TopRight => {
                    let new_right = (mx / w).max(data.left + 0.01);
                    let new_top = (my / h).min(data.bottom - 0.01);
                    setters.set_right(new_right.clamp(0.0, 1.0));
                    setters.set_top(new_top.clamp(0.0, 1.0));
                    data.right = new_right.clamp(0.0, 1.0);
                    data.top = new_top.clamp(0.0, 1.0);
                }
                DragRegion::BottomLeft => {
                    let new_left = (mx / w).min(data.right - 0.01);
                    let new_bottom = (my / h).max(data.top + 0.01);
                    setters.set_left(new_left.clamp(0.0, 1.0));
                    setters.set_bottom(new_bottom.clamp(0.0, 1.0));
                    data.left = new_left.clamp(0.0, 1.0);
                    data.bottom = new_bottom.clamp(0.0, 1.0);
                }
                DragRegion::BottomRight => {
                    let new_right = (mx / w).max(data.left + 0.01);
                    let new_bottom = (my / h).max(data.top + 0.01);
                    setters.set_right(new_right.clamp(0.0, 1.0));
                    setters.set_bottom(new_bottom.clamp(0.0, 1.0));
                    data.right = new_right.clamp(0.0, 1.0);
                    data.bottom = new_bottom.clamp(0.0, 1.0);
                }
                DragRegion::Center => {
                    let dx = (mx - data.drag_offset_x) / w;
                    let dy = (my - data.drag_offset_y) / h;

                    let width = data.right - data.left;
                    let height = data.bottom - data.top;

                    let new_left = (data.left + dx).clamp(0.0, 1.0 - width);
                    let new_top = (data.top + dy).clamp(0.0, 1.0 - height);

                    setters.set_left(new_left);
                    setters.set_right(new_left + width);
                    setters.set_top(new_top);
                    setters.set_bottom(new_top + height);

                    data.left = new_left;
                    data.right = new_left + width;
                    data.top = new_top;
                    data.bottom = new_top + height;

                    // update drag offset for smooth continuous motion
                    data.drag_offset_x = mx;
                    data.drag_offset_y = my;
                }
                _ => {}
            }

            write_all_to_source();
        }
    }

    fn on_mouse_down(&mut self, mx: f32, my: f32, w: f32, h: f32) {
        let region = self.region_under_mouse(mx, my, w, h);
        if region != DragRegion::None {
            let data = self.data_mut();
            data.dragging = true;
            data.drag_region = region;
            data.drag_offset_x = mx;
            data.drag_offset_y = my;
        }
    }

    fn on_mouse_up(&mut self) {
        let data = self.data_mut();
        data.dragging = false;
        data.drag_region = DragRegion::None;
    }

    fn update_cursor_style(&self, state: &mut EngineState, region: DragRegion) {
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



