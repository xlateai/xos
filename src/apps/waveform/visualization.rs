pub fn draw_waveform_red(state: &mut EngineState, samples: &[f32]) {
    let buffer = &mut state.frame.buffer;
    let width = state.frame.width;
    let height = state.frame.height;
    let vertical = height > width;

    let (len, scale, center) = if vertical {
        (height, width as f32 * 0.5 * 0.8, width as f32 * 0.5)
    } else {
        (width, height as f32 * 0.5 * 0.8, height as f32 * 0.5)
    };

    let step = samples.len().max(1) as f32 / len as f32;
    let stride = 2;
    let thickness = 2;

    let mut prev = None;

    for i in (0..len).step_by(stride as usize) {
        let sample_index = (i as f32 * step) as usize;
        if sample_index >= samples.len() {
            break;
        }

        let offset = samples[sample_index] * scale;
        let (x, y) = if vertical {
            ((center + offset) as isize, i as isize)
        } else {
            (i as isize, (center - offset) as isize)
        };

        if let Some((prev_x, prev_y)) = prev {
            // Draw red line
            let dx = (x as isize - prev_x as isize).abs();
            let dy = (y as isize - prev_y as isize).abs();
            let sx = if prev_x < x { 1 } else { -1 };
            let sy = if prev_y < y { 1 } else { -1 };
            let mut err = dx - dy;
            let mut px = prev_x;
            let mut py = prev_y;
            while px != x || py != y {
                for tx in 0..thickness {
                    for ty in 0..thickness {
                        let rx = px + tx as isize;
                        let ry = py + ty as isize;
                        if rx >= 0 && ry >= 0 && (rx as u32) < width && (ry as u32) < height {
                            let idx = (ry as usize * width as usize + rx as usize) * 4;
                            buffer[idx] = 255;
                            buffer[idx + 1] = 0;
                            buffer[idx + 2] = 0;
                            buffer[idx + 3] = 255;
                        }
                    }
                }
                let e2 = 2 * err;
                if e2 > -dy {
                    err -= dy;
                    px += sx;
                }
                if e2 < dx {
                    err += dx;
                    py += sy;
                }
            }
        }
        prev = Some((x, y));
    }
}
use crate::engine::EngineState;
use super::waveform::Waveform;

fn draw_line(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    x0: isize,
    y0: isize,
    x1: isize,
    y1: isize,
    thickness: usize,
) {
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx - dy;
    let mut x = x0;
    let mut y = y0;

    while x != x1 || y != y1 {
        for tx in 0..thickness {
            for ty in 0..thickness {
                let px = x + tx as isize;
                let py = y + ty as isize;
                if px >= 0 && py >= 0 && (px as u32) < width && (py as u32) < height {
                    let i = (py as usize * width as usize + px as usize) * 4;
                    buffer[i] = 0;
                    buffer[i + 1] = 255;
                    buffer[i + 2] = 0;
                    buffer[i + 3] = 255;
                }
            }
        }

        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }
    }
}

pub fn draw_waveform(state: &mut EngineState, samples: &[f32]) {
    let buffer = &mut state.frame.buffer;
    let width = state.frame.width;
    let height = state.frame.height;
    let vertical = height > width;

    let (len, scale, center) = if vertical {
        (height, width as f32 * 0.5 * 0.8, width as f32 * 0.5)
    } else {
        (width, height as f32 * 0.5 * 0.8, height as f32 * 0.5)
    };

    let step = samples.len().max(1) as f32 / len as f32;
    let stride = 2;
    let thickness = 2;

    let mut prev = None;

    for i in (0..len).step_by(stride as usize) {
        let sample_index = (i as f32 * step) as usize;
        if sample_index >= samples.len() {
            break;
        }

        let offset = samples[sample_index] * scale;
        let (x, y) = if vertical {
            ((center + offset) as isize, i as isize)
        } else {
            (i as isize, (center - offset) as isize)
        };

        if let Some((prev_x, prev_y)) = prev {
            draw_line(buffer, width, height, prev_x, prev_y, x, y, thickness);
        }

        prev = Some((x, y));
    }
}

pub fn draw_active_replay_button(waveform: &Waveform, state: &mut EngineState) {
    const BUTTON_SIZE: f32 = 40.0;
    const MARGIN: f32 = 20.0;

    let buffer = &mut state.frame.buffer;
    let width = state.frame.width as f32;
    let height = state.frame.height as f32;

    let x_center = width - MARGIN - BUTTON_SIZE / 2.0;
    let y_center = height - MARGIN - BUTTON_SIZE / 2.0;

    let x0 = (x_center - BUTTON_SIZE / 2.0) as usize;
    let x1 = (x_center + BUTTON_SIZE / 2.0) as usize;
    let y0 = (y_center - BUTTON_SIZE / 2.0) as usize;
    let y1 = (y_center + BUTTON_SIZE / 2.0) as usize;

    let (r, g, b) = if waveform.is_actively_replaying {
        (0, 200, 0)
    } else {
        (60, 60, 60)
    };

    for y in y0..y1.min(state.frame.height as usize) {
        for x in x0..x1.min(state.frame.width as usize) {
            let i = (y * state.frame.width as usize + x) * 4;
            if i + 3 < buffer.len() {
                buffer[i] = r;
                buffer[i + 1] = g;
                buffer[i + 2] = b;
                buffer[i + 3] = 255;
            }
        }
    }
}

pub fn draw_record_button(waveform: &Waveform, state: &mut EngineState) {
    const BUTTON_SIZE: f32 = 40.0;
    const MARGIN: f32 = 20.0;
    const BUTTON_SPACING: f32 = 50.0;
    
    let buffer = &mut state.frame.buffer;
    let width = state.frame.width as f32;
    let height = state.frame.height as f32;

    let x_center = width - MARGIN - BUTTON_SIZE / 2.0 - BUTTON_SPACING;
    let y_center = height - MARGIN - BUTTON_SIZE / 2.0;

    let x0 = (x_center - BUTTON_SIZE / 2.0) as usize;
    let x1 = (x_center + BUTTON_SIZE / 2.0) as usize;
    let y0 = (y_center - BUTTON_SIZE / 2.0) as usize;
    let y1 = (y_center + BUTTON_SIZE / 2.0) as usize;

    let (r, g, b) = if waveform.recording_state.button_pressed || waveform.recording_state.is_recording {
        (255, 50, 50)
    } else {
        (60, 60, 60)
    };

    for y in y0..y1.min(state.frame.height as usize) {
        for x in x0..x1.min(state.frame.width as usize) {
            let i = (y * state.frame.width as usize + x) * 4;
            if i + 3 < buffer.len() {
                buffer[i] = r;
                buffer[i + 1] = g;
                buffer[i + 2] = b;
                buffer[i + 3] = 255;
            }
        }
    }
}

pub fn draw_replay_recording_button(waveform: &Waveform, state: &mut EngineState) {
    const BUTTON_SIZE: f32 = 40.0;
    const MARGIN: f32 = 20.0;
    const BUTTON_SPACING: f32 = 50.0;

    if waveform.recording_state.recorded_samples.is_empty() {
        return;
    }

    let buffer = &mut state.frame.buffer;
    let width = state.frame.width as f32;
    let height = state.frame.height as f32;

    let x_center = width - MARGIN - BUTTON_SIZE / 2.0;
    let y_center = height - MARGIN - BUTTON_SIZE / 2.0 - BUTTON_SPACING;

    let x0 = (x_center - BUTTON_SIZE / 2.0) as usize;
    let x1 = (x_center + BUTTON_SIZE / 2.0) as usize;
    let y0 = (y_center - BUTTON_SIZE / 2.0) as usize;
    let y1 = (y_center + BUTTON_SIZE / 2.0) as usize;

    let (r, g, b) = if waveform.is_replaying_recording {
        (0, 200, 0)
    } else {
        (60, 60, 60)
    };

    for y in y0..y1.min(state.frame.height as usize) {
        for x in x0..x1.min(state.frame.width as usize) {
            let i = (y * state.frame.width as usize + x) * 4;
            if i + 3 < buffer.len() {
                buffer[i] = r;
                buffer[i + 1] = g;
                buffer[i + 2] = b;
                buffer[i + 3] = 255;
            }
        }
    }
}

pub fn is_inside_active_replay_button(mouse_x: f32, mouse_y: f32, state: &EngineState) -> bool {
    const BUTTON_SIZE: f32 = 40.0;
    const MARGIN: f32 = 20.0;

    let width = state.frame.width as f32;
    let height = state.frame.height as f32;

    let x_center = width - MARGIN - BUTTON_SIZE / 2.0;
    let y_center = height - MARGIN - BUTTON_SIZE / 2.0;

    let half_size = BUTTON_SIZE / 2.0;

    mouse_x >= x_center - half_size &&
    mouse_x <= x_center + half_size &&
    mouse_y >= y_center - half_size &&
    mouse_y <= y_center + half_size
}

pub fn is_inside_record_button(mouse_x: f32, mouse_y: f32, state: &EngineState) -> bool {
    const BUTTON_SIZE: f32 = 40.0;
    const MARGIN: f32 = 20.0;
    const BUTTON_SPACING: f32 = 50.0;
    
    let width = state.frame.width as f32;
    let height = state.frame.height as f32;

    let x_center = width - MARGIN - BUTTON_SIZE / 2.0 - BUTTON_SPACING;
    let y_center = height - MARGIN - BUTTON_SIZE / 2.0;

    let half_size = BUTTON_SIZE / 2.0;
    
    mouse_x >= x_center - half_size &&
    mouse_x <= x_center + half_size &&
    mouse_y >= y_center - half_size &&
    mouse_y <= y_center + half_size
}

pub fn is_inside_replay_recording_button(mouse_x: f32, mouse_y: f32, state: &EngineState) -> bool {
    const BUTTON_SIZE: f32 = 40.0;
    const MARGIN: f32 = 20.0;
    const BUTTON_SPACING: f32 = 50.0;

    let width = state.frame.width as f32;
    let height = state.frame.height as f32;

    let x_center = width - MARGIN - BUTTON_SIZE / 2.0;
    let y_center = height - MARGIN - BUTTON_SIZE / 2.0 - BUTTON_SPACING;

    let half_size = BUTTON_SIZE / 2.0;

    mouse_x >= x_center - half_size &&
    mouse_x <= x_center + half_size &&
    mouse_y >= y_center - half_size &&
    mouse_y <= y_center + half_size
}
