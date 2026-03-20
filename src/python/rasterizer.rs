use rustpython_vm::{PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs};
use std::sync::Mutex;
use crate::text::text_rasterization::TextRasterizer;
use fontdue::Font;

// Thread-safe wrapper for raw pointer
pub(crate) struct FrameBufferPtr(pub(crate) *mut u8);
unsafe impl Send for FrameBufferPtr {}
unsafe impl Sync for FrameBufferPtr {}

// Global pointer to the current frame buffer (set during tick)
pub(crate) static CURRENT_FRAME_BUFFER: Mutex<Option<FrameBufferPtr>> = Mutex::new(None);
pub(crate) static CURRENT_FRAME_WIDTH: Mutex<usize> = Mutex::new(0);
pub(crate) static CURRENT_FRAME_HEIGHT: Mutex<usize> = Mutex::new(0);

// Global font for text rasterization (lazy loaded)
static GLOBAL_FONT: Mutex<Option<Font>> = Mutex::new(None);

/// Called by PyApp before tick to set the frame buffer pointer
pub fn set_frame_buffer_context(buffer: &mut [u8], width: usize, height: usize) {
    *CURRENT_FRAME_BUFFER.lock().unwrap() = Some(FrameBufferPtr(buffer.as_mut_ptr()));
    *CURRENT_FRAME_WIDTH.lock().unwrap() = width;
    *CURRENT_FRAME_HEIGHT.lock().unwrap() = height;
}

/// Called by PyApp after tick to clear the frame buffer pointer
pub fn clear_frame_buffer_context() {
    *CURRENT_FRAME_BUFFER.lock().unwrap() = None;
}

/// xos.rasterizer.circles() - efficiently draw circles directly on the Rust frame buffer
/// 
/// Usage: xos.rasterizer.circles(frame, positions, radii, color)
/// - frame: frame object (ignored, we use the global context)
/// - positions: list of (x, y) tuples in pixel coordinates
/// - radii: list of radii in pixels
/// - color: (r, g, b, a) tuple
fn circles(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    // Extract arguments
    let args_vec = args.args;
    if args_vec.len() != 4 {
        return Err(vm.new_type_error(format!(
            "circles() takes exactly 4 arguments ({} given)",
            args_vec.len()
        )));
    }
    
    let _frame_dict = &args_vec[0]; // Ignored, we use global context
    let positions_list = &args_vec[1];
    let radii_list = &args_vec[2];
    let color_tuple = &args_vec[3];
    
    // Get the frame buffer from global context
    let buffer_ptr_opt = CURRENT_FRAME_BUFFER.lock().unwrap().as_ref().map(|ptr| ptr.0);
    let width = *CURRENT_FRAME_WIDTH.lock().unwrap();
    let height = *CURRENT_FRAME_HEIGHT.lock().unwrap();
    
    let buffer_ptr = buffer_ptr_opt.ok_or_else(|| {
        vm.new_runtime_error("No frame buffer context set. Rasterizer must be called during tick().".to_string())
    })?;
    
    // Parse color tuple (r, g, b, a)
    let color_obj = color_tuple.downcast_ref::<rustpython_vm::builtins::PyTuple>()
        .ok_or_else(|| vm.new_type_error("color must be a tuple".to_string()))?;
    let color_vec = color_obj.as_slice();
    if color_vec.len() != 4 {
        return Err(vm.new_type_error("color must be (r, g, b, a)".to_string()));
    }
    let r: i32 = color_vec[0].clone().try_into_value(vm)?;
    let g: i32 = color_vec[1].clone().try_into_value(vm)?;
    let b: i32 = color_vec[2].clone().try_into_value(vm)?;
    let a: i32 = color_vec[3].clone().try_into_value(vm)?;
    let color = (r as u8, g as u8, b as u8, a as u8);
    
    // Get positions list
    let positions = positions_list.downcast_ref::<rustpython_vm::builtins::PyList>()
        .ok_or_else(|| vm.new_type_error("positions must be a list".to_string()))?;
    
    // Get radii list
    let radii = radii_list.downcast_ref::<rustpython_vm::builtins::PyList>()
        .ok_or_else(|| vm.new_type_error("radii must be a list".to_string()))?;
    
    // Collect all circle data first before drawing
    let positions_vec = positions.borrow_vec();
    let radii_vec = radii.borrow_vec();
    
    let mut circles_to_draw = Vec::new();
    
    for (i, pos_obj) in positions_vec.iter().enumerate() {
        // Parse position tuple
        let pos_tuple = pos_obj.downcast_ref::<rustpython_vm::builtins::PyTuple>()
            .ok_or_else(|| vm.new_type_error("position must be a tuple".to_string()))?;
        let pos_vec = pos_tuple.as_slice();
        if pos_vec.len() != 2 {
            return Err(vm.new_type_error("position must be (x, y)".to_string()));
        }
        let cx: f64 = pos_vec[0].clone().try_into_value(vm)?;
        let cy: f64 = pos_vec[1].clone().try_into_value(vm)?;
        
        // Get radius (either from list or use first one for all)
        let radius: f64 = if i < radii_vec.len() {
            radii_vec[i].clone().try_into_value(vm)?
        } else if !radii_vec.is_empty() {
            radii_vec[0].clone().try_into_value(vm)?
        } else {
            return Err(vm.new_type_error("radii list is empty".to_string()));
        };
        
        circles_to_draw.push((cx as f32, cy as f32, radius as f32));
    }
    
    // Drop borrows before drawing
    drop(positions_vec);
    drop(radii_vec);
    
    // Get mutable buffer slice
    let buffer_len = width * height * 4;
    let buffer = unsafe { std::slice::from_raw_parts_mut(buffer_ptr, buffer_len) };
    
    // Now draw all circles directly to the Rust buffer
    for (cx, cy, radius) in circles_to_draw {
        draw_circle_direct(buffer, width, height, cx, cy, radius, color);
    }
    
    Ok(vm.ctx.none())
}

fn draw_circle_direct(
    buffer: &mut [u8],
    width: usize,
    height: usize,
    cx: f32,
    cy: f32,
    radius: f32,
    color: (u8, u8, u8, u8),
) {
    let radius_squared = radius * radius;
    
    let start_x = (cx - radius).max(0.0) as usize;
    let end_x = ((cx + radius + 1.0) as usize).min(width);
    let start_y = (cy - radius).max(0.0) as usize;
    let end_y = ((cy + radius + 1.0) as usize).min(height);
    
    for y in start_y..end_y {
        for x in start_x..end_x {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            if dx * dx + dy * dy <= radius_squared {
                let idx = (y * width + x) * 4;
                if idx + 3 < buffer.len() {
                    buffer[idx + 0] = color.0;
                    buffer[idx + 1] = color.1;
                    buffer[idx + 2] = color.2;
                    buffer[idx + 3] = color.3;
                }
            }
        }
    }
}

/// xos.rasterizer.lines() - efficiently draw lines directly on the Rust frame buffer
/// 
/// Usage: xos.rasterizer.lines(frame, start_points, end_points, thicknesses, color)
/// - frame: frame object (ignored, we use the global context)
/// - start_points: list of (x, y) tuples in pixel coordinates
/// - end_points: list of (x, y) tuples in pixel coordinates
/// - thicknesses: list of line thicknesses in pixels
/// - color: (r, g, b, a) tuple
fn lines(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    // Extract arguments
    let args_vec = args.args;
    if args_vec.len() != 5 {
        return Err(vm.new_type_error(format!(
            "lines() takes exactly 5 arguments ({} given)",
            args_vec.len()
        )));
    }
    
    let _frame_dict = &args_vec[0]; // Ignored, we use global context
    let start_points_list = &args_vec[1];
    let end_points_list = &args_vec[2];
    let thicknesses_list = &args_vec[3];
    let color_tuple = &args_vec[4];
    
    // Get the frame buffer from global context
    let buffer_ptr_opt = CURRENT_FRAME_BUFFER.lock().unwrap().as_ref().map(|ptr| ptr.0);
    let width = *CURRENT_FRAME_WIDTH.lock().unwrap();
    let height = *CURRENT_FRAME_HEIGHT.lock().unwrap();
    
    let buffer_ptr = buffer_ptr_opt.ok_or_else(|| {
        vm.new_runtime_error("No frame buffer context set. Rasterizer must be called during tick().".to_string())
    })?;
    
    // Parse color tuple (r, g, b, a)
    let color_obj = color_tuple.downcast_ref::<rustpython_vm::builtins::PyTuple>()
        .ok_or_else(|| vm.new_type_error("color must be a tuple".to_string()))?;
    let color_vec = color_obj.as_slice();
    if color_vec.len() != 4 {
        return Err(vm.new_type_error("color must be (r, g, b, a)".to_string()));
    }
    let r: i32 = color_vec[0].clone().try_into_value(vm)?;
    let g: i32 = color_vec[1].clone().try_into_value(vm)?;
    let b: i32 = color_vec[2].clone().try_into_value(vm)?;
    let a: i32 = color_vec[3].clone().try_into_value(vm)?;
    let color = (r as u8, g as u8, b as u8, a as u8);
    
    // Get lists
    let start_points = start_points_list.downcast_ref::<rustpython_vm::builtins::PyList>()
        .ok_or_else(|| vm.new_type_error("start_points must be a list".to_string()))?;
    let end_points = end_points_list.downcast_ref::<rustpython_vm::builtins::PyList>()
        .ok_or_else(|| vm.new_type_error("end_points must be a list".to_string()))?;
    let thicknesses = thicknesses_list.downcast_ref::<rustpython_vm::builtins::PyList>()
        .ok_or_else(|| vm.new_type_error("thicknesses must be a list".to_string()))?;
    
    // Collect all line data first before drawing
    let start_points_vec = start_points.borrow_vec();
    let end_points_vec = end_points.borrow_vec();
    let thicknesses_vec = thicknesses.borrow_vec();
    
    let mut lines_to_draw = Vec::new();
    
    for (i, start_obj) in start_points_vec.iter().enumerate() {
        if i >= end_points_vec.len() {
            break;
        }
        
        // Parse start point tuple
        let start_tuple = start_obj.downcast_ref::<rustpython_vm::builtins::PyTuple>()
            .ok_or_else(|| vm.new_type_error("start point must be a tuple".to_string()))?;
        let start_vec = start_tuple.as_slice();
        if start_vec.len() != 2 {
            return Err(vm.new_type_error("start point must be (x, y)".to_string()));
        }
        let x1: f64 = start_vec[0].clone().try_into_value(vm)?;
        let y1: f64 = start_vec[1].clone().try_into_value(vm)?;
        
        // Parse end point tuple
        let end_tuple = end_points_vec[i].downcast_ref::<rustpython_vm::builtins::PyTuple>()
            .ok_or_else(|| vm.new_type_error("end point must be a tuple".to_string()))?;
        let end_vec = end_tuple.as_slice();
        if end_vec.len() != 2 {
            return Err(vm.new_type_error("end point must be (x, y)".to_string()));
        }
        let x2: f64 = end_vec[0].clone().try_into_value(vm)?;
        let y2: f64 = end_vec[1].clone().try_into_value(vm)?;
        
        // Get thickness
        let thickness: f64 = if i < thicknesses_vec.len() {
            thicknesses_vec[i].clone().try_into_value(vm)?
        } else if !thicknesses_vec.is_empty() {
            thicknesses_vec[0].clone().try_into_value(vm)?
        } else {
            return Err(vm.new_type_error("thicknesses list is empty".to_string()));
        };
        
        lines_to_draw.push((x1 as f32, y1 as f32, x2 as f32, y2 as f32, thickness as f32));
    }
    
    // Drop borrows before drawing
    drop(start_points_vec);
    drop(end_points_vec);
    drop(thicknesses_vec);
    
    // Get mutable buffer slice
    let buffer_len = width * height * 4;
    let buffer = unsafe { std::slice::from_raw_parts_mut(buffer_ptr, buffer_len) };
    
    // Now draw all lines directly to the Rust buffer
    for (x1, y1, x2, y2, thickness) in lines_to_draw {
        draw_line_direct(buffer, width, height, x1, y1, x2, y2, thickness, color);
    }
    
    Ok(vm.ctx.none())
}

/// xos.rasterizer.lines_batched() - efficiently draw lines with individual colors
/// 
/// Usage: xos.rasterizer.lines_batched(frame, start_points, end_points, thicknesses, colors)
/// - frame: frame object (ignored, we use the global context)
/// - start_points: list of (x, y) tuples in pixel coordinates
/// - end_points: list of (x, y) tuples in pixel coordinates
/// - thicknesses: list of line thicknesses in pixels
/// - colors: list of (r, g, b, a) tuples (one per line)
fn lines_batched(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    // Extract arguments
    let args_vec = args.args;
    if args_vec.len() != 5 {
        return Err(vm.new_type_error(format!(
            "lines_batched() takes exactly 5 arguments ({} given)",
            args_vec.len()
        )));
    }
    
    let _frame_dict = &args_vec[0]; // Ignored, we use global context
    let start_points_list = &args_vec[1];
    let end_points_list = &args_vec[2];
    let thicknesses_list = &args_vec[3];
    let colors_list = &args_vec[4];
    
    // Get the frame buffer from global context
    let buffer_ptr_opt = CURRENT_FRAME_BUFFER.lock().unwrap().as_ref().map(|ptr| ptr.0);
    let width = *CURRENT_FRAME_WIDTH.lock().unwrap();
    let height = *CURRENT_FRAME_HEIGHT.lock().unwrap();
    
    let buffer_ptr = buffer_ptr_opt.ok_or_else(|| {
        vm.new_runtime_error("No frame buffer context set. Rasterizer must be called during tick().".to_string())
    })?;
    
    // Get lists
    let start_points = start_points_list.downcast_ref::<rustpython_vm::builtins::PyList>()
        .ok_or_else(|| vm.new_type_error("start_points must be a list".to_string()))?;
    let end_points = end_points_list.downcast_ref::<rustpython_vm::builtins::PyList>()
        .ok_or_else(|| vm.new_type_error("end_points must be a list".to_string()))?;
    let thicknesses = thicknesses_list.downcast_ref::<rustpython_vm::builtins::PyList>()
        .ok_or_else(|| vm.new_type_error("thicknesses must be a list".to_string()))?;
    let colors = colors_list.downcast_ref::<rustpython_vm::builtins::PyList>()
        .ok_or_else(|| vm.new_type_error("colors must be a list".to_string()))?;
    
    // Collect all line data first before drawing
    let start_points_vec = start_points.borrow_vec();
    let end_points_vec = end_points.borrow_vec();
    let thicknesses_vec = thicknesses.borrow_vec();
    let colors_vec = colors.borrow_vec();
    
    let mut lines_to_draw = Vec::new();
    
    for (i, start_obj) in start_points_vec.iter().enumerate() {
        if i >= end_points_vec.len() {
            break;
        }
        
        // Parse start point tuple
        let start_tuple = start_obj.downcast_ref::<rustpython_vm::builtins::PyTuple>()
            .ok_or_else(|| vm.new_type_error("start point must be a tuple".to_string()))?;
        let start_vec = start_tuple.as_slice();
        if start_vec.len() != 2 {
            return Err(vm.new_type_error("start point must be (x, y)".to_string()));
        }
        let x1: f64 = start_vec[0].clone().try_into_value(vm)?;
        let y1: f64 = start_vec[1].clone().try_into_value(vm)?;
        
        // Parse end point tuple
        let end_tuple = end_points_vec[i].downcast_ref::<rustpython_vm::builtins::PyTuple>()
            .ok_or_else(|| vm.new_type_error("end point must be a tuple".to_string()))?;
        let end_vec = end_tuple.as_slice();
        if end_vec.len() != 2 {
            return Err(vm.new_type_error("end point must be (x, y)".to_string()));
        }
        let x2: f64 = end_vec[0].clone().try_into_value(vm)?;
        let y2: f64 = end_vec[1].clone().try_into_value(vm)?;
        
        // Get thickness
        let thickness: f64 = if i < thicknesses_vec.len() {
            thicknesses_vec[i].clone().try_into_value(vm)?
        } else if !thicknesses_vec.is_empty() {
            thicknesses_vec[0].clone().try_into_value(vm)?
        } else {
            return Err(vm.new_type_error("thicknesses list is empty".to_string()));
        };
        
        // Parse color tuple for this line
        let color = if i < colors_vec.len() {
            let color_obj = colors_vec[i].downcast_ref::<rustpython_vm::builtins::PyTuple>()
                .ok_or_else(|| vm.new_type_error("color must be a tuple".to_string()))?;
            let color_slice = color_obj.as_slice();
            if color_slice.len() != 4 {
                return Err(vm.new_type_error("color must be (r, g, b, a)".to_string()));
            }
            let r: i32 = color_slice[0].clone().try_into_value(vm)?;
            let g: i32 = color_slice[1].clone().try_into_value(vm)?;
            let b: i32 = color_slice[2].clone().try_into_value(vm)?;
            let a: i32 = color_slice[3].clone().try_into_value(vm)?;
            (r as u8, g as u8, b as u8, a as u8)
        } else {
            return Err(vm.new_type_error("colors list must have same length as points".to_string()));
        };
        
        lines_to_draw.push((x1 as f32, y1 as f32, x2 as f32, y2 as f32, thickness as f32, color));
    }
    
    // Drop borrows before drawing
    drop(start_points_vec);
    drop(end_points_vec);
    drop(thicknesses_vec);
    drop(colors_vec);
    
    // Get mutable buffer slice
    let buffer_len = width * height * 4;
    let buffer = unsafe { std::slice::from_raw_parts_mut(buffer_ptr, buffer_len) };
    
    // Now draw all lines directly to the Rust buffer
    for (x1, y1, x2, y2, thickness, color) in lines_to_draw {
        draw_line_direct(buffer, width, height, x1, y1, x2, y2, thickness, color);
    }
    
    Ok(vm.ctx.none())
}

fn draw_line_direct(
    buffer: &mut [u8],
    width: usize,
    height: usize,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    thickness: f32,
    color: (u8, u8, u8, u8),
) {
    // For very thin lines (< 2 pixels), use super-fast Bresenham algorithm
    if thickness < 2.0 {
        draw_line_bresenham(buffer, width, height, x1, y1, x2, y2, color);
        return;
    }
    
    let radius = thickness / 2.0;
    
    // Calculate line vector and length
    let dx = x2 - x1;
    let dy = y2 - y1;
    let length = (dx * dx + dy * dy).sqrt();
    
    if length < 0.001 {
        // Degenerate line, just draw a circle
        draw_circle_direct(buffer, width, height, x1, y1, radius, color);
        return;
    }
    
    // For thick lines: Draw circles along the line at regular intervals
    // This creates a smooth thick line much faster than checking every pixel
    
    // Calculate number of steps based on thickness (ensure smooth coverage)
    let step_size = (radius * 0.5).max(1.0);
    let num_steps = (length / step_size).ceil() as i32 + 1;
    
    for i in 0..=num_steps {
        let t = (i as f32) / (num_steps as f32);
        let x = x1 + dx * t;
        let y = y1 + dy * t;
        draw_circle_direct(buffer, width, height, x, y, radius, color);
    }
}

/// Ultra-fast Bresenham line algorithm for thin lines (1 pixel)
fn draw_line_bresenham(
    buffer: &mut [u8],
    width: usize,
    height: usize,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    color: (u8, u8, u8, u8),
) {
    let mut x0 = x1 as i32;
    let mut y0 = y1 as i32;
    let x1 = x2 as i32;
    let y1 = y2 as i32;
    
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    
    loop {
        // Draw pixel if in bounds
        if x0 >= 0 && x0 < width as i32 && y0 >= 0 && y0 < height as i32 {
            let idx = (y0 as usize * width + x0 as usize) * 4;
            if idx + 3 < buffer.len() {
                buffer[idx + 0] = color.0;
                buffer[idx + 1] = color.1;
                buffer[idx + 2] = color.2;
                buffer[idx + 3] = color.3;
            }
        }
        
        if x0 == x1 && y0 == y1 {
            break;
        }
        
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

/// xos.rasterizer.draw_image_centered(frame, image)
/// 
/// Clears the frame, then draws the image centered and scaled to fit the shorter screen edge.
/// - frame: frame object (required for API consistency; frame buffer comes from global context)
/// - image: tensor/dict with _data and shape (H, W, C) - e.g. from convolve_image()
fn draw_image_centered(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() != 2 {
        return Err(vm.new_type_error(format!(
            "draw_image_centered() takes exactly 2 arguments (frame, image), got {}",
            args_vec.len()
        )));
    }
    
    let _frame = &args_vec[0]; // Required for API consistency
    let image_arg = &args_vec[1];
    
    // Extract shape (H, W) from image
    let shape = if let Some(dict) = image_arg.downcast_ref::<rustpython_vm::builtins::PyDict>() {
        dict.get_item("shape", vm).ok()
    } else {
        image_arg.get_attr("shape", vm).ok()
    };
    let shape = shape.ok_or_else(|| vm.new_type_error("image must have shape".to_string()))?;
    let slc: &[rustpython_vm::PyObjectRef] = shape.downcast_ref::<rustpython_vm::builtins::PyTuple>()
        .map(|t| t.as_slice())
        .ok_or_else(|| vm.new_type_error("image shape must be a tuple (H, W, ...)".to_string()))?;
    if slc.len() < 2 {
        return Err(vm.new_type_error("image shape must have at least (H, W)".to_string()));
    }
    let img_h: usize = slc[0].clone().try_into_value::<i64>(vm)
        .map_err(|_| vm.new_type_error("shape H must be int".to_string()))? as usize;
    let img_w: usize = slc[1].clone().try_into_value::<i64>(vm)
        .map_err(|_| vm.new_type_error("shape W must be int".to_string()))? as usize;
    
    // Extract _data list from image (dict["_data"] or obj._data or obj._data["_data"])
    let data_obj = if let Some(dict) = image_arg.downcast_ref::<rustpython_vm::builtins::PyDict>() {
        dict.get_item("_data", vm).ok()
    } else if let Ok(data_attr) = image_arg.get_attr("_data", vm) {
        if let Some(inner) = data_attr.downcast_ref::<rustpython_vm::builtins::PyDict>() {
            inner.get_item("_data", vm).ok()
        } else {
            Some(data_attr)
        }
    } else {
        None
    };
    let data_obj = data_obj.ok_or_else(|| vm.new_type_error("image must have _data".to_string()))?;
    let data_list = data_obj.downcast_ref::<rustpython_vm::builtins::PyList>()
        .ok_or_else(|| vm.new_type_error("image _data must be a list".to_string()))?;
    
    let values_vec = data_list.borrow_vec();
    let expected_len = img_w * img_h * 4;
    if values_vec.len() < expected_len {
        return Err(vm.new_value_error(format!(
            "image_data length {} < {} (need {}x{}x4)",
            values_vec.len(), expected_len, img_w, img_h
        )));
    }
    
    // Copy to local u8 buffer (we need to release the PyList borrow)
    let mut img_bytes: Vec<u8> = Vec::with_capacity(expected_len);
    for i in 0..expected_len {
        let v: i32 = values_vec[i].clone().try_into_value(vm)?;
        img_bytes.push(v.clamp(0, 255) as u8);
    }
    drop(values_vec);
    
    // Get frame buffer
    let buffer_ptr_opt = CURRENT_FRAME_BUFFER.lock().unwrap().as_ref().map(|ptr| ptr.0);
    let screen_w = *CURRENT_FRAME_WIDTH.lock().unwrap();
    let screen_h = *CURRENT_FRAME_HEIGHT.lock().unwrap();
    
    let buffer_ptr = buffer_ptr_opt.ok_or_else(|| {
        vm.new_runtime_error("No frame buffer context set. draw_image_centered must be called during tick().".to_string())
    })?;
    
    let buffer_len = screen_w * screen_h * 4;
    let buffer = unsafe { std::slice::from_raw_parts_mut(buffer_ptr, buffer_len) };
    
    // Clear to black
    buffer.fill(0);
    
    // Scale to fit shorter edge, centered
    let scale_w = screen_w as f32 / img_w as f32;
    let scale_h = screen_h as f32 / img_h as f32;
    let scale = scale_w.min(scale_h);
    let sw = (img_w as f32 * scale) as usize;
    let sh = (img_h as f32 * scale) as usize;
    let ox = ((screen_w - sw) / 2) as i32;
    let oy = ((screen_h - sh) / 2) as i32;
    
    for dy in 0..sh {
        let sy = oy + dy as i32;
        if sy < 0 || sy >= screen_h as i32 {
            continue;
        }
        for dx in 0..sw {
            let sx = ox + dx as i32;
            if sx < 0 || sx >= screen_w as i32 {
                continue;
            }
            let src_x = (dx as f32 / scale) as usize;
            let src_y = (dy as f32 / scale) as usize;
            let src_x = src_x.min(img_w.saturating_sub(1));
            let src_y = src_y.min(img_h.saturating_sub(1));
            let src_idx = (src_y * img_w + src_x) * 4;
            let dst_idx = (sy as usize * screen_w + sx as usize) * 4;
            if dst_idx + 3 < buffer.len() && src_idx + 3 < img_bytes.len() {
                buffer[dst_idx + 0] = img_bytes[src_idx + 0];
                buffer[dst_idx + 1] = img_bytes[src_idx + 1];
                buffer[dst_idx + 2] = img_bytes[src_idx + 2];
                buffer[dst_idx + 3] = img_bytes[src_idx + 3];
            }
        }
    }
    
    Ok(vm.ctx.none())
}

/// xos.rasterizer.clear() - clear the frame buffer to black
/// 
/// Efficiently clears the entire frame buffer to black (all zeros)
fn clear(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    // Get the frame buffer from global context
    let buffer_ptr_opt = CURRENT_FRAME_BUFFER.lock().unwrap().as_ref().map(|ptr| ptr.0);
    let width = *CURRENT_FRAME_WIDTH.lock().unwrap();
    let height = *CURRENT_FRAME_HEIGHT.lock().unwrap();
    
    let buffer_ptr = buffer_ptr_opt.ok_or_else(|| {
        vm.new_runtime_error("No frame buffer context set. clear must be called during tick().".to_string())
    })?;
    
    let buffer_len = width * height * 4;
    let buffer = unsafe { std::slice::from_raw_parts_mut(buffer_ptr, buffer_len) };
    
    // Clear to black
    buffer.fill(0);
    
    Ok(vm.ctx.none())
}

/// xos.rasterizer.fill() - fill the entire frame buffer with a solid color
/// 
/// Usage: xos.rasterizer.fill(frame, color)
/// - frame: frame object (ignored, we use the global context)
/// - color: (r, g, b, a) tuple
fn fill(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() != 2 {
        return Err(vm.new_type_error(format!(
            "fill() takes exactly 2 arguments ({} given)",
            args_vec.len()
        )));
    }
    
    let _frame_dict = &args_vec[0]; // Ignored, we use global context
    let color_tuple = &args_vec[1];
    
    // Get the frame buffer from global context
    let buffer_ptr_opt = CURRENT_FRAME_BUFFER.lock().unwrap().as_ref().map(|ptr| ptr.0);
    let width = *CURRENT_FRAME_WIDTH.lock().unwrap();
    let height = *CURRENT_FRAME_HEIGHT.lock().unwrap();
    
    let buffer_ptr = buffer_ptr_opt.ok_or_else(|| {
        vm.new_runtime_error("No frame buffer context set. fill must be called during tick().".to_string())
    })?;
    
    // Parse color tuple (r, g, b, a)
    let color_obj = color_tuple.downcast_ref::<rustpython_vm::builtins::PyTuple>()
        .ok_or_else(|| vm.new_type_error("color must be a tuple".to_string()))?;
    let color_vec = color_obj.as_slice();
    if color_vec.len() != 4 {
        return Err(vm.new_type_error("color must be (r, g, b, a)".to_string()));
    }
    let r: i32 = color_vec[0].clone().try_into_value(vm)?;
    let g: i32 = color_vec[1].clone().try_into_value(vm)?;
    let b: i32 = color_vec[2].clone().try_into_value(vm)?;
    let a: i32 = color_vec[3].clone().try_into_value(vm)?;
    
    let buffer_len = width * height * 4;
    let buffer = unsafe { std::slice::from_raw_parts_mut(buffer_ptr, buffer_len) };
    
    // Fill buffer with color (RGBA pattern)
    for pixel in buffer.chunks_exact_mut(4) {
        pixel[0] = r as u8;
        pixel[1] = g as u8;
        pixel[2] = b as u8;
        pixel[3] = a as u8;
    }
    
    Ok(vm.ctx.none())
}

/// xos.rasterizer._fill_buffer(array_dict, values) - fill buffer 1:1 with values
/// 
/// Internal function to efficiently fill the frame buffer with a list of values
/// This is called by _ArrayWrapper when doing slice assignment: array[:] = values
fn fill_buffer(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() != 2 {
        return Err(vm.new_type_error(format!(
            "_fill_buffer() takes exactly 2 arguments ({} given)",
            args_vec.len()
        )));
    }
    
    let _array_dict = &args_vec[0]; // For future use if needed
    let values_list = &args_vec[1];
    
    // Get the frame buffer from global context
    let buffer_ptr_opt = CURRENT_FRAME_BUFFER.lock().unwrap().as_ref().map(|ptr| ptr.0);
    let width = *CURRENT_FRAME_WIDTH.lock().unwrap();
    let height = *CURRENT_FRAME_HEIGHT.lock().unwrap();
    
    let buffer_ptr = buffer_ptr_opt.ok_or_else(|| {
        vm.new_runtime_error("No frame buffer context set. _fill_buffer must be called during tick().".to_string())
    })?;
    
    let buffer_len = width * height * 4;
    let buffer = unsafe { std::slice::from_raw_parts_mut(buffer_ptr, buffer_len) };
    
    // Parse values - can be a list or an object with _data attribute
    // Get the data attribute upfront if needed (to avoid lifetime issues)
    let data_attr_holder = if values_list.downcast_ref::<rustpython_vm::builtins::PyList>().is_none() {
        vm.get_attribute_opt(values_list.clone(), "_data")
            .ok()
            .flatten()
    } else {
        None
    };
    
    // Now get the actual list
    let actual_list = if let Some(list) = values_list.downcast_ref::<rustpython_vm::builtins::PyList>() {
        list
    } else if let Some(ref data_obj) = data_attr_holder {
        data_obj.downcast_ref::<rustpython_vm::builtins::PyList>()
            .ok_or_else(|| vm.new_type_error("_data must be a list".to_string()))?
    } else {
        return Err(vm.new_type_error("values must be a list or have _data attribute".to_string()));
    };
    
    let values_vec = actual_list.borrow_vec();
    
    // Copy values 1:1 into buffer
    let copy_len = values_vec.len().min(buffer_len);
    for i in 0..copy_len {
        let val: i32 = values_vec[i].clone().try_into_value(vm)?;
        buffer[i] = val.clamp(0, 255) as u8;
    }
    
    Ok(vm.ctx.none())
}

/// xos.rasterizer.rects_filled() - ultra-fast vectorized rectangle drawing
/// 
/// Usage (waterfall mode): xos.rasterizer.rects_filled(frame, color_rows, num_bins, pixel_width, pixel_height, num_rows)
/// Usage (single rect): xos.rasterizer.rects_filled(frame, x1, y1, x2, y2, color)
/// 
/// Waterfall mode: draws a grid of pixels from color rows (fills entire screen)
/// Single rect mode: draws one rectangle (numpy-style compatibility)
fn rects_filled(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    
    // Get frame buffer
    let buffer_ptr_opt = CURRENT_FRAME_BUFFER.lock().unwrap().as_ref().map(|ptr| ptr.0);
    let width = *CURRENT_FRAME_WIDTH.lock().unwrap();
    let height = *CURRENT_FRAME_HEIGHT.lock().unwrap();
    
    let buffer_ptr = buffer_ptr_opt.ok_or_else(|| {
        vm.new_runtime_error("No frame buffer context set".to_string())
    })?;
    
    let buffer_len = width * height * 4;
    let buffer = unsafe { std::slice::from_raw_parts_mut(buffer_ptr, buffer_len) };
    
    // Check mode based on argument count
    if args_vec.len() == 6 && args_vec[1].downcast_ref::<rustpython_vm::builtins::PyList>().is_some() {
        // WATERFALL MODE: (frame, color_rows, num_bins, pixel_width, pixel_height, num_rows)
        // Note: pixel_width and pixel_height are passed but not used - we calculate exact boundaries
        let color_rows_list = &args_vec[1];
        let num_bins: i32 = args_vec[2].clone().try_into_value(vm)?;
        let _pixel_width: i32 = args_vec[3].clone().try_into_value(vm)?;
        let _pixel_height: i32 = args_vec[4].clone().try_into_value(vm)?;
        let num_rows: i32 = args_vec[5].clone().try_into_value(vm)?;
        
        // Parse color rows
        let rows = color_rows_list.downcast_ref::<rustpython_vm::builtins::PyList>()
            .ok_or_else(|| vm.new_type_error("color_rows must be a list".to_string()))?;
        let rows_vec = rows.borrow_vec();
        
        // Draw each row (vectorized in Rust)
        // Calculate exact boundaries to fill entire screen with no gaps
        for row_idx in 0..num_rows.min(rows_vec.len() as i32) {
            let color_row = rows_vec[row_idx as usize].downcast_ref::<rustpython_vm::builtins::PyList>()
                .ok_or_else(|| vm.new_type_error("each row must be a list".to_string()))?;
            let colors_vec = color_row.borrow_vec();
            
            // Calculate exact row boundaries: last row extends to height
            let y_start = (row_idx as usize * height) / num_rows as usize;
            let y_end = ((row_idx + 1) as usize * height) / num_rows as usize;
            
            // Draw each bin in this row
            for bin_idx in 0..num_bins.min(colors_vec.len() as i32) {
                // Parse color
                let color_tuple = colors_vec[bin_idx as usize].downcast_ref::<rustpython_vm::builtins::PyTuple>()
                    .ok_or_else(|| vm.new_type_error("color must be tuple".to_string()))?;
                let color_slice = color_tuple.as_slice();
                if color_slice.len() != 4 {
                    continue;
                }
                let r: i32 = color_slice[0].clone().try_into_value(vm)?;
                let g: i32 = color_slice[1].clone().try_into_value(vm)?;
                let b: i32 = color_slice[2].clone().try_into_value(vm)?;
                let a: i32 = color_slice[3].clone().try_into_value(vm)?;
                
                // Calculate exact bin boundaries: last bin extends to width
                let x_start = (bin_idx as usize * width) / num_bins as usize;
                let x_end = ((bin_idx + 1) as usize * width) / num_bins as usize;
                
                // Fill rectangle (optimized: fill row by row)
                for y in y_start..y_end.min(height) {
                    let row_start = (y * width + x_start) * 4;
                    let row_end = (y * width + x_end.min(width)) * 4;
                    
                    // Fill this row of the rectangle
                    let mut idx = row_start;
                    while idx < row_end && idx + 3 < buffer.len() {
                        buffer[idx] = r as u8;
                        buffer[idx + 1] = g as u8;
                        buffer[idx + 2] = b as u8;
                        buffer[idx + 3] = a as u8;
                        idx += 4;
                    }
                }
            }
        }
    } else if args_vec.len() == 6 {
        // SINGLE RECT MODE: (frame, x1, y1, x2, y2, color)
        let x1: i32 = args_vec[1].clone().try_into_value(vm)?;
        let y1: i32 = args_vec[2].clone().try_into_value(vm)?;
        let x2: i32 = args_vec[3].clone().try_into_value(vm)?;
        let y2: i32 = args_vec[4].clone().try_into_value(vm)?;
        let color_tuple = &args_vec[5];
        
        // Parse color
        let color_obj = color_tuple.downcast_ref::<rustpython_vm::builtins::PyTuple>()
            .ok_or_else(|| vm.new_type_error("color must be a tuple".to_string()))?;
        let color_vec = color_obj.as_slice();
        if color_vec.len() != 4 {
            return Err(vm.new_type_error("color must be (r, g, b, a)".to_string()));
        }
        let r: i32 = color_vec[0].clone().try_into_value(vm)?;
        let g: i32 = color_vec[1].clone().try_into_value(vm)?;
        let b: i32 = color_vec[2].clone().try_into_value(vm)?;
        let a: i32 = color_vec[3].clone().try_into_value(vm)?;
        
        // Clamp and draw
        let x_start = x1.max(0).min(width as i32) as usize;
        let x_end = x2.max(0).min(width as i32) as usize;
        let y_start = y1.max(0).min(height as i32) as usize;
        let y_end = y2.max(0).min(height as i32) as usize;
        
        for y in y_start..y_end {
            let row_start = (y * width + x_start) * 4;
            let row_end = (y * width + x_end) * 4;
            let mut idx = row_start;
            while idx < row_end && idx + 3 < buffer.len() {
                buffer[idx] = r as u8;
                buffer[idx + 1] = g as u8;
                buffer[idx + 2] = b as u8;
                buffer[idx + 3] = a as u8;
                idx += 4;
            }
        }
    } else {
        return Err(vm.new_type_error(format!(
            "rects_filled() takes 6 arguments (waterfall: frame, color_rows, num_bins, pixel_width, pixel_height, num_rows) or (single rect: frame, x1, y1, x2, y2, color), got {}",
            args_vec.len()
        )));
    }
    
    Ok(vm.ctx.none())
}

/// xos.rasterizer.text() - render text on the frame buffer
/// 
/// Usage: xos.rasterizer.text(text, x, y, font_size, color, max_width)
/// - text: string to render
/// - x: x position in pixels
/// - y: y position in pixels (top of text)
/// - font_size: font size in pixels
/// - color: (r, g, b) or (r, g, b, a) tuple
/// - max_width: optional maximum width for text wrapping (defaults to screen width)
fn text(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() < 5 || args_vec.len() > 6 {
        return Err(vm.new_type_error(format!(
            "text() takes 5 or 6 arguments ({} given)",
            args_vec.len()
        )));
    }
    
    // Extract arguments
    let text_str: String = args_vec[0].clone().try_into_value(vm)?;
    let x: f64 = args_vec[1].clone().try_into_value(vm)?;
    let y: f64 = args_vec[2].clone().try_into_value(vm)?;
    let font_size: f64 = args_vec[3].clone().try_into_value(vm)?;
    let color_tuple = &args_vec[4];
    
    // Get the frame buffer from global context
    let buffer_ptr_opt = CURRENT_FRAME_BUFFER.lock().unwrap().as_ref().map(|ptr| ptr.0);
    let width = *CURRENT_FRAME_WIDTH.lock().unwrap();
    let height = *CURRENT_FRAME_HEIGHT.lock().unwrap();
    
    let buffer_ptr = buffer_ptr_opt.ok_or_else(|| {
        vm.new_runtime_error("No frame buffer context set. Rasterizer must be called during tick().".to_string())
    })?;
    
    // Parse color tuple (r, g, b) or (r, g, b, a)
    let color_obj = color_tuple.downcast_ref::<rustpython_vm::builtins::PyTuple>()
        .ok_or_else(|| vm.new_type_error("color must be a tuple".to_string()))?;
    let color_vec = color_obj.as_slice();
    if color_vec.len() != 3 && color_vec.len() != 4 {
        return Err(vm.new_type_error("color must be (r, g, b) or (r, g, b, a)".to_string()));
    }
    let r: i32 = color_vec[0].clone().try_into_value(vm)?;
    let g: i32 = color_vec[1].clone().try_into_value(vm)?;
    let b: i32 = color_vec[2].clone().try_into_value(vm)?;
    let a: i32 = if color_vec.len() == 4 {
        color_vec[3].clone().try_into_value(vm)?
    } else {
        255
    };
    
    // Get max_width (optional)
    let max_width = if args_vec.len() == 6 {
        let mw: f64 = args_vec[5].clone().try_into_value(vm)?;
        mw as f32
    } else {
        width as f32
    };
    
    // Load font if not already loaded
    let mut font_lock = GLOBAL_FONT.lock().unwrap();
    if font_lock.is_none() {
        // Load default font (NotoSans-Medium)
        let font_data = include_bytes!("../../assets/NotoSans-Medium.ttf");
        match Font::from_bytes(font_data as &[u8], fontdue::FontSettings::default()) {
            Ok(font) => *font_lock = Some(font),
            Err(e) => return Err(vm.new_runtime_error(format!("Failed to load font: {}", e))),
        }
    }
    let font = font_lock.as_ref().unwrap();
    
    // Create text rasterizer
    let mut rasterizer = TextRasterizer::new(font.clone(), font_size as f32);
    rasterizer.set_text(text_str);
    rasterizer.tick(max_width, height as f32);
    
    // Draw characters to buffer
    let buffer = unsafe { std::slice::from_raw_parts_mut(buffer_ptr, width * height * 4) };
    
    for character in &rasterizer.characters {
        let char_x = x as i32 + character.x as i32;
        let char_y = y as i32 + character.y as i32;
        
        for bitmap_y in 0..character.metrics.height {
            for bitmap_x in 0..character.metrics.width {
                let alpha = character.bitmap[bitmap_y * character.metrics.width + bitmap_x];
                
                if alpha == 0 {
                    continue;
                }
                
                let px = char_x + bitmap_x as i32;
                let py = char_y + bitmap_y as i32;
                
                if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                    let idx = ((py as usize * width + px as usize) * 4) as usize;
                    
                    // Blend with existing pixel using alpha
                    let alpha_f = (alpha as f32 / 255.0) * (a as f32 / 255.0);
                    let inv_alpha = 1.0 - alpha_f;
                    
                    buffer[idx + 0] = ((r as f32 * alpha_f) + (buffer[idx + 0] as f32 * inv_alpha)) as u8;
                    buffer[idx + 1] = ((g as f32 * alpha_f) + (buffer[idx + 1] as f32 * inv_alpha)) as u8;
                    buffer[idx + 2] = ((b as f32 * alpha_f) + (buffer[idx + 2] as f32 * inv_alpha)) as u8;
                    buffer[idx + 3] = 255; // Keep alpha at full
                }
            }
        }
    }
    
    Ok(vm.ctx.none())
}

pub fn make_rasterizer_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.rasterizer", vm.ctx.new_dict(), None);
    module.set_attr("circles", vm.new_function("circles", circles), vm).unwrap();
    module.set_attr("lines", vm.new_function("lines", lines), vm).unwrap();
    module.set_attr("lines_batched", vm.new_function("lines_batched", lines_batched), vm).unwrap();
    module.set_attr("clear", vm.new_function("clear", clear), vm).unwrap();
    module.set_attr("fill", vm.new_function("fill", fill), vm).unwrap();
    module.set_attr("rects_filled", vm.new_function("rects_filled", rects_filled), vm).unwrap();
    module.set_attr("_fill_buffer", vm.new_function("_fill_buffer", fill_buffer), vm).unwrap();
    module.set_attr("text", vm.new_function("text", text), vm).unwrap();
    module.set_attr("draw_image_centered", vm.new_function("draw_image_centered", draw_image_centered), vm).unwrap();
    module
}
