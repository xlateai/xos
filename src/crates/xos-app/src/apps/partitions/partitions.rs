use xos_core::engine::{Application, EngineState};
use xos_core::tuneables;
use super::partition::{Partition, PartitionSetters, DragRegion};
use super::partition_viewports::{PartitionA, PartitionB, PartitionC};

// Rectangle 0
tuneables! {
    rect0_left: f32 = 0.18222387;
    rect0_right: f32 = 0.48222387;
    rect0_top: f32 = 0.16110823;
    rect0_bottom: f32 = 0.5111084;
}

// Rectangle 1
tuneables! {
    rect1_left: f32 = 0.5177809;
    rect1_right: f32 = 0.8177809;
    rect1_top: f32 = 0.11311597;
    rect1_bottom: f32 = 0.5131161;
}

// Rectangle 2
tuneables! {
    rect2_left: f32 = 0.2345478;
    rect2_right: f32 = 0.7345477;
    rect2_top: f32 = 0.57679886;
    rect2_bottom: f32 = 0.82679886;
}

const BACKGROUND_COLOR: (u8, u8, u8) = (32, 32, 32);

struct Rect0Setters;

impl PartitionSetters for Rect0Setters {
    fn set_left(&self, v: f32) {
        rect0_left().set(v);
    }
    fn set_right(&self, v: f32) {
        rect0_right().set(v);
    }
    fn set_top(&self, v: f32) {
        rect0_top().set(v);
    }
    fn set_bottom(&self, v: f32) {
        rect0_bottom().set(v);
    }
}

struct Rect1Setters;

impl PartitionSetters for Rect1Setters {
    fn set_left(&self, v: f32) {
        rect1_left().set(v);
    }
    fn set_right(&self, v: f32) {
        rect1_right().set(v);
    }
    fn set_top(&self, v: f32) {
        rect1_top().set(v);
    }
    fn set_bottom(&self, v: f32) {
        rect1_bottom().set(v);
    }
}

struct Rect2Setters;

impl PartitionSetters for Rect2Setters {
    fn set_left(&self, v: f32) {
        rect2_left().set(v);
    }
    fn set_right(&self, v: f32) {
        rect2_right().set(v);
    }
    fn set_top(&self, v: f32) {
        rect2_top().set(v);
    }
    fn set_bottom(&self, v: f32) {
        rect2_bottom().set(v);
    }
}

pub struct Partitions {
    partitions: Vec<Box<dyn Partition>>,
}

impl Partitions {
    pub fn new() -> Self {
        Self {
            partitions: vec![
                Box::new(PartitionA::new(
                    rect0_left().get(),
                    rect0_right().get(),
                    rect0_top().get(),
                    rect0_bottom().get(),
                )),
                Box::new(PartitionB::new(
                    rect1_left().get(),
                    rect1_right().get(),
                    rect1_top().get(),
                    rect1_bottom().get(),
                )),
                Box::new(PartitionC::new(
                    rect2_left().get(),
                    rect2_right().get(),
                    rect2_top().get(),
                    rect2_bottom().get(),
                )),
            ],
        }
    }

    fn sync_partition_from_tuneables(&mut self, idx: usize) {
        match idx {
            0 => {
                self.partitions[0].set_left(rect0_left().get());
                self.partitions[0].set_right(rect0_right().get());
                self.partitions[0].set_top(rect0_top().get());
                self.partitions[0].set_bottom(rect0_bottom().get());
            }
            1 => {
                self.partitions[1].set_left(rect1_left().get());
                self.partitions[1].set_right(rect1_right().get());
                self.partitions[1].set_top(rect1_top().get());
                self.partitions[1].set_bottom(rect1_bottom().get());
            }
            2 => {
                self.partitions[2].set_left(rect2_left().get());
                self.partitions[2].set_right(rect2_right().get());
                self.partitions[2].set_top(rect2_top().get());
                self.partitions[2].set_bottom(rect2_bottom().get());
            }
            _ => {}
        }
    }
}

impl Application for Partitions {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        // Sync partition positions from tuneables (in case they were changed externally)
        for i in 0..self.partitions.len() {
            self.sync_partition_from_tuneables(i);
        }

        state.frame_buffer_mut().chunks_exact_mut(4).for_each(|p| {
            p[0] = BACKGROUND_COLOR.0;
            p[1] = BACKGROUND_COLOR.1;
            p[2] = BACKGROUND_COLOR.2;
            p[3] = 0xff;
        });

        let shape = state.frame.shape();
        let width = shape[1] as u32;
        let height = shape[0] as u32;
        let buffer = state.frame_buffer_mut();
        for partition in &self.partitions {
            partition.draw(buffer, width, height);
        }
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        let mx = state.mouse.x;
        let my = state.mouse.y;
        let shape = state.frame.shape();
        let w = shape[1] as f32;
        let h = shape[0] as f32;

        // Check if any partition is being dragged
        let mut any_dragging = false;
        for (idx, partition) in self.partitions.iter_mut().enumerate() {
            if partition.dragging() {
                any_dragging = true;
                match idx {
                    0 => {
                        partition.on_mouse_move(mx, my, w, h, &Rect0Setters);
                    }
                    1 => {
                        partition.on_mouse_move(mx, my, w, h, &Rect1Setters);
                    }
                    2 => {
                        partition.on_mouse_move(mx, my, w, h, &Rect2Setters);
                    }
                    _ => {}
                }
                break;
            }
        }

        if !any_dragging {
            // Update cursor style based on which partition is under the mouse
            let mut found = false;
            for partition in &self.partitions {
                let region = partition.region_under_mouse(mx, my, w, h);
                if region != DragRegion::None {
                    partition.update_cursor_style(state, region);
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
        let shape = state.frame.shape();
        let w = shape[1] as f32;
        let h = shape[0] as f32;
        let mx = state.mouse.x;
        let my = state.mouse.y;

        // Find which partition (if any) is under the mouse
        for partition in &mut self.partitions {
            partition.on_mouse_down(mx, my, w, h);
            if partition.dragging() {
                break; // Only allow dragging one partition at a time
            }
        }
    }

    fn on_mouse_up(&mut self, _state: &mut EngineState) {
        for partition in &mut self.partitions {
            partition.on_mouse_up();
        }
    }
}