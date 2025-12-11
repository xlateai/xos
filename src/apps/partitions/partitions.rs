use crate::engine::{Application, EngineState};
use crate::tuneables;
use super::partition::{Partition, DragRegion};

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

pub struct Partitions {
    partitions: Vec<Partition>,
}

impl Partitions {
    pub fn new() -> Self {
        Self {
            partitions: vec![
                Partition::new(
                    rect0_left().get(),
                    rect0_right().get(),
                    rect0_top().get(),
                    rect0_bottom().get(),
                    RECT_COLOR_0,
                ),
                Partition::new(
                    rect1_left().get(),
                    rect1_right().get(),
                    rect1_top().get(),
                    rect1_bottom().get(),
                    RECT_COLOR_1,
                ),
                Partition::new(
                    rect2_left().get(),
                    rect2_right().get(),
                    rect2_top().get(),
                    rect2_bottom().get(),
                    RECT_COLOR_2,
                ),
            ],
        }
    }

    fn sync_partition_from_tuneables(&mut self, idx: usize) {
        match idx {
            0 => {
                self.partitions[0].left = rect0_left().get();
                self.partitions[0].right = rect0_right().get();
                self.partitions[0].top = rect0_top().get();
                self.partitions[0].bottom = rect0_bottom().get();
            }
            1 => {
                self.partitions[1].left = rect1_left().get();
                self.partitions[1].right = rect1_right().get();
                self.partitions[1].top = rect1_top().get();
                self.partitions[1].bottom = rect1_bottom().get();
            }
            2 => {
                self.partitions[2].left = rect2_left().get();
                self.partitions[2].right = rect2_right().get();
                self.partitions[2].top = rect2_top().get();
                self.partitions[2].bottom = rect2_bottom().get();
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

        state.frame.buffer.chunks_exact_mut(4).for_each(|p| {
            p[0] = BACKGROUND_COLOR.0;
            p[1] = BACKGROUND_COLOR.1;
            p[2] = BACKGROUND_COLOR.2;
            p[3] = 0xff;
        });

        for partition in &self.partitions {
            partition.draw(
                &mut state.frame.buffer,
                state.frame.width,
                state.frame.height,
            );
        }
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        let mx = state.mouse.x;
        let my = state.mouse.y;
        let w = state.frame.width as f32;
        let h = state.frame.height as f32;

        // Check if any partition is being dragged
        let mut any_dragging = false;
        for (idx, partition) in self.partitions.iter_mut().enumerate() {
            if partition.dragging {
                any_dragging = true;
                match idx {
                    0 => {
                        partition.on_mouse_move(
                            mx, my, w, h,
                            |v| rect0_left().set(v),
                            |v| rect0_right().set(v),
                            |v| rect0_top().set(v),
                            |v| rect0_bottom().set(v),
                        );
                    }
                    1 => {
                        partition.on_mouse_move(
                            mx, my, w, h,
                            |v| rect1_left().set(v),
                            |v| rect1_right().set(v),
                            |v| rect1_top().set(v),
                            |v| rect1_bottom().set(v),
                        );
                    }
                    2 => {
                        partition.on_mouse_move(
                            mx, my, w, h,
                            |v| rect2_left().set(v),
                            |v| rect2_right().set(v),
                            |v| rect2_top().set(v),
                            |v| rect2_bottom().set(v),
                        );
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
        let w = state.frame.width as f32;
        let h = state.frame.height as f32;
        let mx = state.mouse.x;
        let my = state.mouse.y;

        // Find which partition (if any) is under the mouse
        for partition in &mut self.partitions {
            partition.on_mouse_down(mx, my, w, h);
            if partition.dragging {
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
