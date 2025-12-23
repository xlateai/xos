use rustpython_vm::{PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs};
use std::sync::Mutex;

// Thread-safe wrapper for raw pointer
pub(crate) struct FrameBufferPtr(pub(crate) *mut u8);
unsafe impl Send for FrameBufferPtr {}
unsafe impl Sync for FrameBufferPtr {}

// Global pointer to the current frame buffer (set during tick)
pub(crate) static CURRENT_FRAME_BUFFER: Mutex<Option<FrameBufferPtr>> = Mutex::new(None);
pub(crate) static CURRENT_FRAME_WIDTH: Mutex<usize> = Mutex::new(0);
pub(crate) static CURRENT_FRAME_HEIGHT: Mutex<usize> = Mutex::new(0);

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
    
    // Parse values list
    let values = values_list.downcast_ref::<rustpython_vm::builtins::PyList>()
        .ok_or_else(|| vm.new_type_error("values must be a list".to_string()))?;
    
    let values_vec = values.borrow_vec();
    
    // Copy values 1:1 into buffer
    let copy_len = values_vec.len().min(buffer_len);
    for i in 0..copy_len {
        let val: i32 = values_vec[i].clone().try_into_value(vm)?;
        buffer[i] = val.clamp(0, 255) as u8;
    }
    
    Ok(vm.ctx.none())
}

pub fn make_rasterizer_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.rasterizer", vm.ctx.new_dict(), None);
    module.set_attr("circles", vm.new_function("circles", circles), vm).unwrap();
    module.set_attr("lines", vm.new_function("lines", lines), vm).unwrap();
    module.set_attr("clear", vm.new_function("clear", clear), vm).unwrap();
    module.set_attr("_fill_buffer", vm.new_function("_fill_buffer", fill_buffer), vm).unwrap();
    module
}
