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

    // Increased padding for smoother anti-aliasing
    let padding = if anti_alias { 4 } else { 0 };
    let aa_range = 2.25; // Anti-aliasing range in pixels (reduced by ~10%)

    for dy in -radius_i - padding..=radius_i + padding {
        for dx in -radius_i - padding..=radius_i + padding {
            let px = center_x_i + dx;
            let py = center_y_i + dy;

            if px < 0 || py < 0 || (px as u32) >= width || (py as u32) >= height {
                continue;
            }

            let dist_sq = (dx * dx + dy * dy) as f32;
            let dist = dist_sq.sqrt();

            let idx = ((py as u32 * width + px as u32) * 4) as usize;
            if idx + 3 >= buffer.len() {
                continue;
            }

            if anti_alias {
                // Smooth anti-aliasing with wider range
                let edge_dist = radius_f - dist;
                let alpha = if edge_dist > aa_range {
                    // Fully inside
                    1.0
                } else if edge_dist > -aa_range {
                    // In the anti-aliasing zone (inside or outside)
                    // Use smoothstep for smoother falloff: 3t^2 - 2t^3
                    let t = ((edge_dist + aa_range) / (2.0 * aa_range)).max(0.0).min(1.0);
                    let smooth_alpha = t * t * (3.0 - 2.0 * t);
                    smooth_alpha
                } else {
                    // Fully outside
                    0.0
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
                // No anti-aliasing - simple inside/outside check
                if dist <= radius_f {
                    buffer[idx + 0] = color.0;
                    buffer[idx + 1] = color.1;
                    buffer[idx + 2] = color.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }
    }
}
