//! CPU raster primitives split by shape family.

pub mod basic_shapes;
pub mod circles;
pub mod lines;
pub mod niche_shapes;
pub mod rectangles;
pub mod triangles;

pub use basic_shapes::draw_circle;
pub use circles::{
    circles, draw_circle_cpu, draw_circles_cpu, draw_circles_cpu_instances,
};
pub use lines::{draw_line_bresenham, draw_line_direct};
pub use niche_shapes::draw_play_button;
pub use rectangles::{fill_rect, fill_rect_buffer};
pub use triangles::{edge_ori, fill_triangle_buffer, triangles, triangles_buffer};
