// Shared edge test for Delaunay / winding checks (see `rasterizer::shapes::triangles::fill_triangle_buffer` for fills).

use delaunator::Point;

#[inline]
pub fn edge_function(a: &Point, b: &Point, x: f64, y: f64) -> f64 {
    (b.x - a.x) * (y - a.y) - (b.y - a.y) * (x - a.x)
}
