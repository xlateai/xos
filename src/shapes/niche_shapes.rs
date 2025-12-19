/// Niche shape drawing functions for specific use cases

/// Apply a simple box blur to a buffer
fn apply_box_blur(
    src: &[u8],
    dst: &mut [u8],
    width: u32,
    height: u32,
    radius: i32,
) {
    let radius = radius.max(1);
    
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let mut sum_r = 0.0f32;
            let mut sum_g = 0.0f32;
            let mut sum_b = 0.0f32;
            let mut sum_a = 0.0f32;
            let mut count = 0;
            
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let px = x + dx;
                    let py = y + dy;
                    
                    if px >= 0 && py >= 0 && (px as u32) < width && (py as u32) < height {
                        let idx = ((py as u32 * width + px as u32) * 4) as usize;
                        if idx + 3 < src.len() {
                            sum_r += src[idx + 0] as f32;
                            sum_g += src[idx + 1] as f32;
                            sum_b += src[idx + 2] as f32;
                            sum_a += src[idx + 3] as f32;
                            count += 1;
                        }
                    }
                }
            }
            
            let idx = ((y as u32 * width + x as u32) * 4) as usize;
            if idx + 3 < dst.len() && count > 0 {
                dst[idx + 0] = (sum_r / count as f32) as u8;
                dst[idx + 1] = (sum_g / count as f32) as u8;
                dst[idx + 2] = (sum_b / count as f32) as u8;
                dst[idx + 3] = (sum_a / count as f32) as u8;
            }
        }
    }
}

/// Draw a play button icon (isosceles triangle pointing right)
/// The triangle is centered at (center_x, center_y) with the specified width and height
pub fn draw_play_button(
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
    let half_height = triangle_height / 2.0;

    // Triangle vertices (isosceles triangle pointing right):
    // To center the triangle properly, the centroid should be at (center_x, center_y)
    // Centroid = ((left_x + left_x + tip_x) / 3, (top_y + bottom_y + tip_y) / 3)
    // For centroid at (center_x, center_y):
    // (2*left_x + tip_x) / 3 = center_x  =>  2*left_x + tip_x = 3*center_x
    // (top_y + bottom_y + tip_y) / 3 = center_y  =>  top_y + bottom_y + tip_y = 3*center_y
    // Since tip_y = center_y: top_y + bottom_y = 2*center_y
    // With symmetric heights: top_y = center_y - half_height, bottom_y = center_y + half_height ✓
    // For x: tip_x - left_x = triangle_width, and 2*left_x + tip_x = 3*center_x
    // Solving: left_x = center_x - triangle_width/3, tip_x = center_x + 2*triangle_width/3
    let left_x = center_x - triangle_width / 3.0;
    let tip_x = center_x + 2.0 * triangle_width / 3.0;
    let tip_y = center_y;
    let top_y = center_y - half_height;
    let bottom_y = center_y + half_height;

    let min_x = (left_x - 3.0).floor() as i32; // Add padding for blur
    let max_x = (tip_x + 3.0).ceil() as i32;
    let min_y = (top_y - 3.0).floor() as i32;
    let max_y = (bottom_y + 3.0).ceil() as i32;
    
    let region_width = (max_x - min_x + 1) as u32;
    let region_height = (max_y - min_y + 1) as u32;
    
    if anti_alias {
        // Create temporary buffer for triangle rasterization
        let temp_size = (region_width * region_height * 4) as usize;
        let mut temp_buffer = vec![0u8; temp_size];
        
        // Rasterize triangle to temp buffer (no anti-aliasing yet)
        for py in min_y..=max_y {
            for px in min_x..=max_x {
                let px_f = px as f32 + 0.5;
                let py_f = py as f32 + 0.5;

                // Check if point is inside triangle using edge function method
                let edge_ab = (left_x - left_x) * (py_f - top_y) - (bottom_y - top_y) * (px_f - left_x);
                let edge_bc = (tip_x - left_x) * (py_f - bottom_y) - (tip_y - bottom_y) * (px_f - left_x);
                let edge_ca = (left_x - tip_x) * (py_f - tip_y) - (top_y - tip_y) * (px_f - tip_x);
                
                let inside = (edge_ab >= 0.0 && edge_bc >= 0.0 && edge_ca >= 0.0) ||
                            (edge_ab <= 0.0 && edge_bc <= 0.0 && edge_ca <= 0.0);

                if inside {
                    let local_x = (px - min_x) as u32;
                    let local_y = (py - min_y) as u32;
                    let idx = ((local_y * region_width + local_x) * 4) as usize;
                    if idx + 3 < temp_buffer.len() {
                        temp_buffer[idx + 0] = color.0;
                        temp_buffer[idx + 1] = color.1;
                        temp_buffer[idx + 2] = color.2;
                        temp_buffer[idx + 3] = 255;
                    }
                }
            }
        }
        
        // Apply blur to temp buffer (reduced by ~10%)
        let mut blurred_buffer = vec![0u8; temp_size];
        apply_box_blur(&temp_buffer, &mut blurred_buffer, region_width, region_height, 1);
        
        // Composite blurred triangle onto main buffer
        for py in min_y..=max_y {
            for px in min_x..=max_x {
                if px >= 0 && py >= 0 && (px as u32) < width && (py as u32) < height {
                    let local_x = (px - min_x) as u32;
                    let local_y = (py - min_y) as u32;
                    let temp_idx = ((local_y * region_width + local_x) * 4) as usize;
                    let main_idx = ((py as u32 * width + px as u32) * 4) as usize;
                    
                    if temp_idx + 3 < blurred_buffer.len() && main_idx + 3 < buffer.len() {
                        let alpha = blurred_buffer[temp_idx + 3] as f32 / 255.0;
                        let bg_r = buffer[main_idx + 0] as f32;
                        let bg_g = buffer[main_idx + 1] as f32;
                        let bg_b = buffer[main_idx + 2] as f32;
                        
                        buffer[main_idx + 0] = ((blurred_buffer[temp_idx + 0] as f32 * alpha) + (bg_r * (1.0 - alpha))) as u8;
                        buffer[main_idx + 1] = ((blurred_buffer[temp_idx + 1] as f32 * alpha) + (bg_g * (1.0 - alpha))) as u8;
                        buffer[main_idx + 2] = ((blurred_buffer[temp_idx + 2] as f32 * alpha) + (bg_b * (1.0 - alpha))) as u8;
                        buffer[main_idx + 3] = 0xff;
                    }
                }
            }
        }
    } else {
        // No anti-aliasing - just draw directly
        for py in min_y..=max_y {
            for px in min_x..=max_x {
                if px < 0 || py < 0 || (px as u32) >= width || (py as u32) >= height {
                    continue;
                }

                let px_f = px as f32 + 0.5;
                let py_f = py as f32 + 0.5;

                let edge_ab = (left_x - left_x) * (py_f - top_y) - (bottom_y - top_y) * (px_f - left_x);
                let edge_bc = (tip_x - left_x) * (py_f - bottom_y) - (tip_y - bottom_y) * (px_f - left_x);
                let edge_ca = (left_x - tip_x) * (py_f - tip_y) - (top_y - tip_y) * (px_f - tip_x);
                
                let inside = (edge_ab >= 0.0 && edge_bc >= 0.0 && edge_ca >= 0.0) ||
                            (edge_ab <= 0.0 && edge_bc <= 0.0 && edge_ca <= 0.0);

                if inside {
                    let idx = ((py as u32 * width + px as u32) * 4) as usize;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = color.0;
                        buffer[idx + 1] = color.1;
                        buffer[idx + 2] = color.2;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
        }
    }
}

