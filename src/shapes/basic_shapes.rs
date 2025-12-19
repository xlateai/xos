/// Basic shape drawing functions with optional anti-aliasing

/// Draw a circle with optional anti-aliasing
pub fn draw_circle(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    center_x: f32,
    center_y: f32,
    radius: f32,
    color: (u8, u8, u8),
    anti_alias: bool,
) {
    if radius <= 0.0 {
        return;
    }

    let center_x_i = center_x as i32;
    let center_y_i = center_y as i32;
    let radius_i = radius.ceil() as i32;
    let radius_f = radius;

    let padding = if anti_alias { 2 } else { 0 };

    for dy in -radius_i - padding..=radius_i + padding {
        for dx in -radius_i - padding..=radius_i + padding {
            let px = center_x_i + dx;
            let py = center_y_i + dy;
            
            if px < 0 || py < 0 || (px as u32) >= width || (py as u32) >= height {
                continue;
            }

            let dist_sq = (dx * dx + dy * dy) as f32;
            let dist = dist_sq.sqrt();
            
            if dist <= radius_f {
                let idx = ((py as u32 * width + px as u32) * 4) as usize;
                if idx + 3 >= buffer.len() {
                    continue;
                }

                if anti_alias {
                    // Anti-aliasing at edges
                    let edge_dist = radius_f - dist;
                    let alpha = if edge_dist < 1.0 {
                        edge_dist.max(0.0).min(1.0)
                    } else {
                        1.0
                    };

                    // Blend with background
                    let bg_r = buffer[idx + 0] as f32;
                    let bg_g = buffer[idx + 1] as f32;
                    let bg_b = buffer[idx + 2] as f32;

                    buffer[idx + 0] = ((color.0 as f32 * alpha) + (bg_r * (1.0 - alpha))) as u8;
                    buffer[idx + 1] = ((color.1 as f32 * alpha) + (bg_g * (1.0 - alpha))) as u8;
                    buffer[idx + 2] = ((color.2 as f32 * alpha) + (bg_b * (1.0 - alpha))) as u8;
                    buffer[idx + 3] = 0xff;
                } else {
                    buffer[idx + 0] = color.0;
                    buffer[idx + 1] = color.1;
                    buffer[idx + 2] = color.2;
                    buffer[idx + 3] = 0xff;
                }
            } else if anti_alias && dist < radius_f + 1.0 {
                // Anti-aliasing for edge pixels just outside the circle
                let edge_dist = (radius_f + 1.0) - dist;
                let alpha = edge_dist.max(0.0).min(1.0);

                let idx = ((py as u32 * width + px as u32) * 4) as usize;
                if idx + 3 < buffer.len() {
                    let bg_r = buffer[idx + 0] as f32;
                    let bg_g = buffer[idx + 1] as f32;
                    let bg_b = buffer[idx + 2] as f32;

                    buffer[idx + 0] = ((color.0 as f32 * alpha) + (bg_r * (1.0 - alpha))) as u8;
                    buffer[idx + 1] = ((color.1 as f32 * alpha) + (bg_g * (1.0 - alpha))) as u8;
                    buffer[idx + 2] = ((color.2 as f32 * alpha) + (bg_b * (1.0 - alpha))) as u8;
                    buffer[idx + 3] = 0xff;
                }
            }
        }
    }
}

/// Draw an isosceles triangle pointing right (play icon style)
/// The triangle is centered at (center_x, center_y) with the specified width and height
pub fn draw_triangle_right(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    center_x: f32,
    center_y: f32,
    triangle_width: f32,
    triangle_height: f32,
    color: (u8, u8, u8),
    anti_alias: bool,
) {
    let half_width = triangle_width / 2.0;
    let half_height = triangle_height / 2.0;

    // Triangle vertices (isosceles triangle pointing right):
    // Left-top: (center_x - half_width, center_y - half_height)
    // Left-bottom: (center_x - half_width, center_y + half_height)
    // Right point: (center_x + half_width, center_y)
    let left_x = center_x - half_width;
    let tip_x = center_x + half_width;
    let tip_y = center_y;
    let top_y = center_y - half_height;
    let bottom_y = center_y + half_height;

    let min_x = (left_x - 1.0).floor() as i32;
    let max_x = (tip_x + 1.0).ceil() as i32;
    let min_y = (top_y - 1.0).floor() as i32;
    let max_y = (bottom_y + 1.0).ceil() as i32;

    for py in min_y..=max_y {
        for px in min_x..=max_x {
            if px < 0 || py < 0 || (px as u32) >= width || (py as u32) >= height {
                continue;
            }

            let px_f = px as f32 + 0.5;
            let py_f = py as f32 + 0.5;

            // Check if point is inside triangle using edge function method
            // Triangle vertices: A(left_x, top_y), B(left_x, bottom_y), C(tip_x, tip_y)
            // Point is inside if all edge functions have the same sign
            
            // Edge function: (B.x - A.x) * (P.y - A.y) - (B.y - A.y) * (P.x - A.x)
            // Edge AB: from (left_x, top_y) to (left_x, bottom_y)
            let edge_ab = (left_x - left_x) * (py_f - top_y) - (bottom_y - top_y) * (px_f - left_x);
            // Edge BC: from (left_x, bottom_y) to (tip_x, tip_y)
            let edge_bc = (tip_x - left_x) * (py_f - bottom_y) - (tip_y - bottom_y) * (px_f - left_x);
            // Edge CA: from (tip_x, tip_y) to (left_x, top_y)
            let edge_ca = (left_x - tip_x) * (py_f - tip_y) - (top_y - tip_y) * (px_f - tip_x);
            
            // Point is inside if all edges have same sign (all positive or all negative)
            let inside = (edge_ab >= 0.0 && edge_bc >= 0.0 && edge_ca >= 0.0) ||
                        (edge_ab <= 0.0 && edge_bc <= 0.0 && edge_ca <= 0.0);

            if inside {
                let idx = ((py as u32 * width + px as u32) * 4) as usize;
                if idx + 3 >= buffer.len() {
                    continue;
                }

                if anti_alias {
                    // Calculate distance to nearest edge for anti-aliasing
                    // Edge 1: left vertical edge from (left_x, top_y) to (left_x, bottom_y)
                    let dist_to_left_edge = (px_f - left_x).abs();
                    // Edge 2: top diagonal from (left_x, top_y) to (tip_x, tip_y)
                    let dist_to_top_edge = {
                        let edge_dist = ((tip_x - left_x) * (top_y - py_f) - (tip_y - top_y) * (left_x - px_f)).abs()
                            / ((tip_x - left_x).powi(2) + (tip_y - top_y).powi(2)).sqrt();
                        edge_dist
                    };
                    // Edge 3: bottom diagonal from (left_x, bottom_y) to (tip_x, tip_y)
                    let dist_to_bottom_edge = {
                        let edge_dist = ((tip_x - left_x) * (py_f - bottom_y) - (tip_y - bottom_y) * (left_x - px_f)).abs()
                            / ((tip_x - left_x).powi(2) + (tip_y - bottom_y).powi(2)).sqrt();
                        edge_dist
                    };

                    let min_edge_dist = dist_to_left_edge
                        .min(dist_to_top_edge)
                        .min(dist_to_bottom_edge);

                    let alpha = if min_edge_dist < 1.0 {
                        min_edge_dist.max(0.0).min(1.0)
                    } else {
                        1.0
                    };

                    // Blend with background
                    let bg_r = buffer[idx + 0] as f32;
                    let bg_g = buffer[idx + 1] as f32;
                    let bg_b = buffer[idx + 2] as f32;

                    buffer[idx + 0] = ((color.0 as f32 * alpha) + (bg_r * (1.0 - alpha))) as u8;
                    buffer[idx + 1] = ((color.1 as f32 * alpha) + (bg_g * (1.0 - alpha))) as u8;
                    buffer[idx + 2] = ((color.2 as f32 * alpha) + (bg_b * (1.0 - alpha))) as u8;
                    buffer[idx + 3] = 0xff;
                } else {
                    buffer[idx + 0] = color.0;
                    buffer[idx + 1] = color.1;
                    buffer[idx + 2] = color.2;
                    buffer[idx + 3] = 0xff;
                }
            } else if anti_alias {
                // Check if point is near an edge for anti-aliasing
                // Edge 1: left vertical edge
                let dist_to_left_edge = (px_f - left_x).abs();
                // Edge 2: top diagonal
                let dist_to_top_edge = {
                    let edge_dist = ((tip_x - left_x) * (top_y - py_f) - (tip_y - top_y) * (left_x - px_f)).abs()
                        / ((tip_x - left_x).powi(2) + (tip_y - top_y).powi(2)).sqrt();
                    edge_dist
                };
                // Edge 3: bottom diagonal
                let dist_to_bottom_edge = {
                    let edge_dist = ((tip_x - left_x) * (py_f - bottom_y) - (tip_y - bottom_y) * (left_x - px_f)).abs()
                        / ((tip_x - left_x).powi(2) + (tip_y - bottom_y).powi(2)).sqrt();
                    edge_dist
                };

                let min_edge_dist = dist_to_left_edge
                    .min(dist_to_top_edge)
                    .min(dist_to_bottom_edge);

                if min_edge_dist < 1.0 {
                    let alpha = (1.0 - min_edge_dist).max(0.0).min(1.0);

                    let idx = ((py as u32 * width + px as u32) * 4) as usize;
                    if idx + 3 < buffer.len() {
                        let bg_r = buffer[idx + 0] as f32;
                        let bg_g = buffer[idx + 1] as f32;
                        let bg_b = buffer[idx + 2] as f32;

                        buffer[idx + 0] = ((color.0 as f32 * alpha) + (bg_r * (1.0 - alpha))) as u8;
                        buffer[idx + 1] = ((color.1 as f32 * alpha) + (bg_g * (1.0 - alpha))) as u8;
                        buffer[idx + 2] = ((color.2 as f32 * alpha) + (bg_b * (1.0 - alpha))) as u8;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
        }
    }
}

