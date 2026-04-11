use rustpython_vm::{
    PyObjectRef, PyResult, VirtualMachine,
    builtins::{PyBytes, PyDict, PyList, PyModule},
    function::FuncArgs,
    PyRef, TryIntoValue,
};
use std::sync::Mutex;
use crate::python_api::tensors::{tensor_flat_data_list, tensor_shape_tuple};
use crate::rasterizer::shapes::lines::draw_line_direct;
use crate::rasterizer::text::text_rasterization::TextRasterizer;
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

/// Fill a contiguous RGBA8 buffer (`len` must be a multiple of 4). Used by [`crate::rasterizer::fill`]
/// and [`fill`] (Python `xos.rasterizer.fill`).
pub(crate) fn fill_buffer_solid_rgba(buffer: &mut [u8], r: u8, g: u8, b: u8, a: u8) {
    let px = [r, g, b, a];
    for chunk in buffer.chunks_exact_mut(4) {
        chunk.copy_from_slice(&px);
    }
}

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
    
    let mut circles_to_draw = Vec::new();

    if let Some(positions) = positions_list.downcast_ref::<PyList>() {
        let radii = radii_list
            .downcast_ref::<PyList>()
            .ok_or_else(|| vm.new_type_error("radii must be a list".to_string()))?;
        let positions_vec = positions.borrow_vec();
        let radii_vec = radii.borrow_vec();
        for (i, pos_obj) in positions_vec.iter().enumerate() {
            let pos_tuple = pos_obj.downcast_ref::<rustpython_vm::builtins::PyTuple>()
                .ok_or_else(|| vm.new_type_error("position must be a tuple".to_string()))?;
            let pos_vec = pos_tuple.as_slice();
            if pos_vec.len() != 2 {
                return Err(vm.new_type_error("position must be (x, y)".to_string()));
            }
            let cx: f64 = pos_vec[0].clone().try_into_value(vm)?;
            let cy: f64 = pos_vec[1].clone().try_into_value(vm)?;
            let radius: f64 = if i < radii_vec.len() {
                radii_vec[i].clone().try_into_value(vm)?
            } else if !radii_vec.is_empty() {
                radii_vec[0].clone().try_into_value(vm)?
            } else {
                return Err(vm.new_type_error("radii list is empty".to_string()));
            };
            circles_to_draw.push((cx as f32, cy as f32, radius as f32));
        }
    } else {
        let pos_flat = tensor_flat_data_list(positions_list, vm)?;
        let rad_flat = tensor_flat_data_list(radii_list, vm)?;
        let pos_shape = tensor_shape_tuple(positions_list, vm)?;
        if pos_shape.len() != 2 || pos_shape[1] != 2 {
            return Err(vm.new_type_error("positions tensor must be shape (N, 2)".to_string()));
        }
        let n = pos_shape[0];
        if rad_flat.len() != n && rad_flat.len() != 1 {
            return Err(vm.new_type_error("radii tensor must be length N or 1".to_string()));
        }
        for i in 0..n {
            let cx = pos_flat[2 * i] as f64;
            let cy = pos_flat[2 * i + 1] as f64;
            let radius: f64 = if rad_flat.len() == n {
                rad_flat[i] as f64
            } else {
                rad_flat[0] as f64
            };
            circles_to_draw.push((cx as f32, cy as f32, radius as f32));
        }
    }
    
    // Get mutable buffer slice
    let buffer_len = width * height * 4;
    let buffer = unsafe { std::slice::from_raw_parts_mut(buffer_ptr, buffer_len) };
    
    let c = [color.0, color.1, color.2, color.3];
    let instances: Vec<(f32, f32, f32, [u8; 4])> = circles_to_draw
        .iter()
        .map(|&(cx, cy, r)| (cx, cy, r, c))
        .collect();
    crate::rasterizer::draw_circles_cpu_instances(buffer, width, height, &instances);

    Ok(vm.ctx.none())
}

/// xos.rasterizer.triangles(frame, points, colors)
///
/// - `points`: list of `(x, y)` for each vertex; length must be a multiple of 3 (triangles are `a,b,c` repeated).
/// - `colors`: list of `(r, g, b, a)` per triangle, or one tuple broadcast to all triangles.
fn triangles_py(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() != 3 {
        return Err(vm.new_type_error(format!(
            "triangles() takes exactly 3 arguments ({} given)",
            args_vec.len()
        )));
    }

    let _frame = &args_vec[0];
    let points_list = &args_vec[1];
    let colors_list = &args_vec[2];

    let buffer_ptr_opt = CURRENT_FRAME_BUFFER.lock().unwrap().as_ref().map(|ptr| ptr.0);
    let width = *CURRENT_FRAME_WIDTH.lock().unwrap();
    let height = *CURRENT_FRAME_HEIGHT.lock().unwrap();

    let buffer_ptr = buffer_ptr_opt.ok_or_else(|| {
        vm.new_runtime_error(
            "No frame buffer context set. Rasterizer must be called during tick().".to_string(),
        )
    })?;

    let positions = points_list
        .downcast_ref::<rustpython_vm::builtins::PyList>()
        .ok_or_else(|| vm.new_type_error("points must be a list".to_string()))?;
    let colors = colors_list
        .downcast_ref::<rustpython_vm::builtins::PyList>()
        .ok_or_else(|| vm.new_type_error("colors must be a list".to_string()))?;

    let positions_vec = positions.borrow_vec();
    let colors_vec = colors.borrow_vec();

    if positions_vec.len() % 3 != 0 {
        return Err(vm.new_type_error(
            "points length must be divisible by 3 (a,b,c per triangle)".to_string(),
        ));
    }
    let n_tri = positions_vec.len() / 3;
    if n_tri == 0 {
        return Ok(vm.ctx.none());
    }
    if colors_vec.is_empty() {
        return Err(vm.new_type_error("colors is empty".to_string()));
    }
    if colors_vec.len() != n_tri && colors_vec.len() != 1 {
        return Err(vm.new_type_error(format!(
            "colors length {} must be {} (one per triangle) or 1",
            colors_vec.len(),
            n_tri
        )));
    }

    let mut points_flat: Vec<(f32, f32)> = Vec::with_capacity(positions_vec.len());
    for pos_obj in positions_vec.iter() {
        let pos_tuple = pos_obj
            .downcast_ref::<rustpython_vm::builtins::PyTuple>()
            .ok_or_else(|| vm.new_type_error("each point must be (x, y)".to_string()))?;
        let pos_vec = pos_tuple.as_slice();
        if pos_vec.len() != 2 {
            return Err(vm.new_type_error("each point must be (x, y)".to_string()));
        }
        let x: f64 = pos_vec[0].clone().try_into_value(vm)?;
        let y: f64 = pos_vec[1].clone().try_into_value(vm)?;
        points_flat.push((x as f32, y as f32));
    }

    let mut rgba: Vec<[u8; 4]> = Vec::with_capacity(n_tri);
    if colors_vec.len() == 1 {
        let color_obj = colors_vec[0]
            .downcast_ref::<rustpython_vm::builtins::PyTuple>()
            .ok_or_else(|| vm.new_type_error("color must be a tuple".to_string()))?;
        let color_slice = color_obj.as_slice();
        if color_slice.len() != 4 {
            return Err(vm.new_type_error("color must be (r, g, b, a)".to_string()));
        }
        let r: i32 = color_slice[0].clone().try_into_value(vm)?;
        let g: i32 = color_slice[1].clone().try_into_value(vm)?;
        let b: i32 = color_slice[2].clone().try_into_value(vm)?;
        let a: i32 = color_slice[3].clone().try_into_value(vm)?;
        let c = [r as u8, g as u8, b as u8, a as u8];
        rgba.resize(n_tri, c);
    } else {
        for i in 0..n_tri {
            let color_obj = colors_vec[i]
                .downcast_ref::<rustpython_vm::builtins::PyTuple>()
                .ok_or_else(|| vm.new_type_error("color must be a tuple".to_string()))?;
            let color_slice = color_obj.as_slice();
            if color_slice.len() != 4 {
                return Err(vm.new_type_error("color must be (r, g, b, a)".to_string()));
            }
            let r: i32 = color_slice[0].clone().try_into_value(vm)?;
            let g: i32 = color_slice[1].clone().try_into_value(vm)?;
            let b: i32 = color_slice[2].clone().try_into_value(vm)?;
            let a: i32 = color_slice[3].clone().try_into_value(vm)?;
            rgba.push([r as u8, g as u8, b as u8, a as u8]);
        }
    }

    drop(positions_vec);
    drop(colors_vec);

    let buffer_len = width * height * 4;
    let buffer = unsafe { std::slice::from_raw_parts_mut(buffer_ptr, buffer_len) };

    if let Err(e) = crate::rasterizer::triangles_buffer(buffer, width, height, &points_flat, &rgba)
    {
        return Err(vm.new_runtime_error(e));
    }

    Ok(vm.ctx.none())
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
    
    let mut lines_to_draw = Vec::new();

    if let Some(start_points) = start_points_list.downcast_ref::<PyList>() {
        let end_points = end_points_list
            .downcast_ref::<PyList>()
            .ok_or_else(|| vm.new_type_error("end_points must be a list".to_string()))?;
        let thicknesses = thicknesses_list
            .downcast_ref::<PyList>()
            .ok_or_else(|| vm.new_type_error("thicknesses must be a list".to_string()))?;
        let start_points_vec = start_points.borrow_vec();
        let end_points_vec = end_points.borrow_vec();
        let thicknesses_vec = thicknesses.borrow_vec();
        for (i, start_obj) in start_points_vec.iter().enumerate() {
            if i >= end_points_vec.len() {
                break;
            }
            let start_tuple = start_obj.downcast_ref::<rustpython_vm::builtins::PyTuple>()
                .ok_or_else(|| vm.new_type_error("start point must be a tuple".to_string()))?;
            let start_vec = start_tuple.as_slice();
            if start_vec.len() != 2 {
                return Err(vm.new_type_error("start point must be (x, y)".to_string()));
            }
            let x1: f64 = start_vec[0].clone().try_into_value(vm)?;
            let y1: f64 = start_vec[1].clone().try_into_value(vm)?;
            let end_tuple = end_points_vec[i].downcast_ref::<rustpython_vm::builtins::PyTuple>()
                .ok_or_else(|| vm.new_type_error("end point must be a tuple".to_string()))?;
            let end_vec = end_tuple.as_slice();
            if end_vec.len() != 2 {
                return Err(vm.new_type_error("end point must be (x, y)".to_string()));
            }
            let x2: f64 = end_vec[0].clone().try_into_value(vm)?;
            let y2: f64 = end_vec[1].clone().try_into_value(vm)?;
            let thickness: f64 = if i < thicknesses_vec.len() {
                thicknesses_vec[i].clone().try_into_value(vm)?
            } else if !thicknesses_vec.is_empty() {
                thicknesses_vec[0].clone().try_into_value(vm)?
            } else {
                return Err(vm.new_type_error("thicknesses list is empty".to_string()));
            };
            lines_to_draw.push((x1 as f32, y1 as f32, x2 as f32, y2 as f32, thickness as f32));
        }
    } else {
        let sflat = tensor_flat_data_list(start_points_list, vm)?;
        let eflat = tensor_flat_data_list(end_points_list, vm)?;
        let tflat = tensor_flat_data_list(thicknesses_list, vm)?;
        let sshape = tensor_shape_tuple(start_points_list, vm)?;
        if sshape.len() != 2 || sshape[1] != 2 {
            return Err(vm.new_type_error("start_points tensor must be shape (N, 2)".to_string()));
        }
        let n = sshape[0];
        if sflat.len() != eflat.len() {
            return Err(vm.new_type_error("start/end tensor size mismatch".to_string()));
        }
        for i in 0..n {
            let x1 = sflat[2 * i] as f64;
            let y1 = sflat[2 * i + 1] as f64;
            let x2 = eflat[2 * i] as f64;
            let y2 = eflat[2 * i + 1] as f64;
            let thickness: f64 = if tflat.len() == n {
                tflat[i] as f64
            } else if !tflat.is_empty() {
                tflat[0] as f64
            } else {
                return Err(vm.new_type_error("thicknesses tensor is empty".to_string()));
            };
            lines_to_draw.push((x1 as f32, y1 as f32, x2 as f32, y2 as f32, thickness as f32));
        }
    }
    
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
    // Opaque black (not RGBA=0, which is transparent and breaks compositing / FPS overlay).
    fill_buffer_solid_rgba(buffer, 0, 0, 0, 0xff);
    
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
    
    fill_buffer_solid_rgba(buffer, r as u8, g as u8, b as u8, a as u8);
    
    Ok(vm.ctx.none())
}

/// xos.rasterizer._fill_buffer(array_dict, values) - fill buffer 1:1 with values
/// 
/// Internal function to efficiently fill the frame buffer with a list of values
/// This is called by _TensorWrapper when doing slice assignment: array[:] = values
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
    
    // Parse values - supports list, _TensorWrapper, or dict-like tensor data.
    // Walk nested _data/data containers until we reach a flat list.
    let mut cur = values_list.clone();
    let mut depth = 0usize;
    let actual_list = loop {
        if let Some(list) = cur.downcast_ref::<rustpython_vm::builtins::PyList>() {
            break list;
        }

        if depth >= 8 {
            return Err(vm.new_type_error("values nesting too deep while resolving _data".to_string()));
        }

        if let Some(dict) = cur.downcast_ref::<rustpython_vm::builtins::PyDict>() {
            if let Ok(next) = dict.get_item("_data", vm) {
                cur = next;
                depth += 1;
                continue;
            }
            if let Ok(next) = dict.get_item("data", vm) {
                cur = next;
                depth += 1;
                continue;
            }
        }

        if let Ok(Some(next)) = vm.get_attribute_opt(cur.clone(), "_data") {
            cur = next;
            depth += 1;
            continue;
        }

        return Err(vm.new_type_error("values must be a list or tensor-like object with _data list".to_string()));
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

/// xos.rasterizer.rectangles() - draw rectangle outlines from normalized boxes
///
/// Usage: xos.rasterizer.rectangles(frame, boxes, color, thickness=1.0)
/// - frame: frame object (ignored, we use global context)
/// - boxes: either shape (N, 2, 2) or a single box shape (2, 2), normalized [0,1]
/// - color: (r, g, b) or (r, g, b, a)
/// - thickness: optional outline thickness in pixels
fn rectangles(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() < 3 || args_vec.len() > 4 {
        return Err(vm.new_type_error(format!(
            "rectangles() takes 3 or 4 arguments ({} given)",
            args_vec.len()
        )));
    }

    let boxes_obj = &args_vec[1];
    let color_tuple = &args_vec[2];
    let thickness: f32 = if args_vec.len() > 3 {
        args_vec[3].clone().try_into_value::<f64>(vm).unwrap_or(1.0) as f32
    } else {
        1.0
    };

    let buffer_ptr_opt = CURRENT_FRAME_BUFFER.lock().unwrap().as_ref().map(|ptr| ptr.0);
    let width = *CURRENT_FRAME_WIDTH.lock().unwrap();
    let height = *CURRENT_FRAME_HEIGHT.lock().unwrap();
    let buffer_ptr = buffer_ptr_opt.ok_or_else(|| {
        vm.new_runtime_error("No frame buffer context set. rectangles must be called during tick().".to_string())
    })?;
    let buffer = unsafe { std::slice::from_raw_parts_mut(buffer_ptr, width * height * 4) };

    let color_obj = color_tuple
        .downcast_ref::<rustpython_vm::builtins::PyTuple>()
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
    let color = (
        r.clamp(0, 255) as u8,
        g.clamp(0, 255) as u8,
        b.clamp(0, 255) as u8,
        a.clamp(0, 255) as u8,
    );

    let flat = tensor_flat_data_list(boxes_obj, vm)?;
    let shape = tensor_shape_tuple(boxes_obj, vm).unwrap_or_default();

    let mut draw_box = |x1n: f32, y1n: f32, x2n: f32, y2n: f32| {
        let x1 = (x1n.clamp(0.0, 1.0) * width as f32) as f32;
        let y1 = (y1n.clamp(0.0, 1.0) * height as f32) as f32;
        let x2 = (x2n.clamp(0.0, 1.0) * width as f32) as f32;
        let y2 = (y2n.clamp(0.0, 1.0) * height as f32) as f32;
        let (xa, xb) = if x1 <= x2 { (x1, x2) } else { (x2, x1) };
        let (ya, yb) = if y1 <= y2 { (y1, y2) } else { (y2, y1) };

        draw_line_direct(buffer, width, height, xa, ya, xb, ya, thickness, color);
        draw_line_direct(buffer, width, height, xb, ya, xb, yb, thickness, color);
        draw_line_direct(buffer, width, height, xb, yb, xa, yb, thickness, color);
        draw_line_direct(buffer, width, height, xa, yb, xa, ya, thickness, color);
    };

    if shape == vec![2, 2] && flat.len() >= 4 {
        draw_box(flat[0], flat[1], flat[2], flat[3]);
        return Ok(vm.ctx.none());
    }
    if shape.len() == 3 && shape[1] == 2 && shape[2] == 2 {
        let n = shape[0];
        for i in 0..n {
            let base = i * 4;
            if base + 3 >= flat.len() {
                break;
            }
            draw_box(flat[base], flat[base + 1], flat[base + 2], flat[base + 3]);
        }
        return Ok(vm.ctx.none());
    }
    if flat.len() >= 4 && flat.len() % 4 == 0 {
        for chunk in flat.chunks_exact(4) {
            draw_box(chunk[0], chunk[1], chunk[2], chunk[3]);
        }
        return Ok(vm.ctx.none());
    }

    Err(vm.new_type_error(
        "boxes must be shape (2,2), (N,2,2), or flat length multiple of 4".to_string(),
    ))
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
    let font_size: f64 = if let Ok(v) = args_vec[3].clone().try_into_value::<f64>(vm) {
        v
    } else if let Ok(v) = args_vec[3].clone().try_into_value::<i64>(vm) {
        v as f64
    } else {
        return Err(vm.new_type_error("font_size must be an int or float".to_string()));
    };
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
        let font_data = include_bytes!("../core/assets/NotoSans-Medium.ttf");
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

fn blit_rgba_stretch(src: &[u8], sw: usize, sh: usize, dst: &mut [u8], dst_w: usize, dst_h: usize) {
    if src.len() != sw * sh * 4 {
        return;
    }
    for dy in 0..dst_h {
        for dx in 0..dst_w {
            let sx = (dx * sw) / dst_w.max(1);
            let sy = (dy * sh) / dst_h.max(1);
            let si = (sy * sw + sx) * 4;
            let di = (dy * dst_w + dx) * 4;
            if si + 3 < src.len() && di + 3 < dst.len() {
                dst[di..di + 4].copy_from_slice(&src[si..si + 4]);
            }
        }
    }
}

fn frame_object_to_rgba(vm: &VirtualMachine, frame_obj: PyObjectRef) -> PyResult<(usize, usize, Vec<u8>)> {
    let data_obj = if let Ok(d) = vm.get_attribute(frame_obj.clone(), "_data") {
        d
    } else {
        frame_obj
    };
    let dict = data_obj.downcast_ref::<PyDict>().ok_or_else(|| {
        vm.new_type_error("frame_in_frame: src must be a Frame (e.g. mesh remote_frame)".to_string())
    })?;
    let width: usize = dict.get_item("width", vm)?.try_into_value(vm)?;
    let height: usize = dict.get_item("height", vm)?.try_into_value(vm)?;
    let tensor = dict.get_item("tensor", vm)?;
    let tdict = tensor.downcast_ref::<PyDict>().ok_or_else(|| {
        vm.new_type_error("frame_in_frame: expected tensor dict on Frame".to_string())
    })?;
    let data_obj = tdict.get_item("_data", vm)?;
    if let Some(bytes) = data_obj.downcast_ref::<PyBytes>() {
        let s = bytes.as_bytes();
        if s.len() != width * height * 4 {
            return Err(vm.new_value_error("frame tensor byte length mismatch (expect RGBA)".to_string()));
        }
        return Ok((width, height, s.to_vec()));
    }
    Err(vm.new_type_error(
        "frame_in_frame: tensor _data must be bytes (decoded remote frame)".to_string(),
    ))
}

/// Stretch-blit a source [`Frame`] (RGBA bytes) into the active engine framebuffer.
fn frame_in_frame(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() != 2 {
        return Err(vm.new_type_error("frame_in_frame(dst, src) expects 2 arguments".to_string()));
    }
    let src = args_vec[1].clone();
    let (sw, sh, src_rgba) = frame_object_to_rgba(vm, src)?;

    let buffer_ptr_opt = CURRENT_FRAME_BUFFER.lock().unwrap().as_ref().map(|ptr| ptr.0);
    let dst_w = *CURRENT_FRAME_WIDTH.lock().unwrap();
    let dst_h = *CURRENT_FRAME_HEIGHT.lock().unwrap();
    let buffer_ptr = buffer_ptr_opt.ok_or_else(|| {
        vm.new_runtime_error(
            "frame_in_frame: no active framebuffer (call during tick() with the engine)".to_string(),
        )
    })?;
    let dst_len = dst_w.saturating_mul(dst_h).saturating_mul(4);
    let dst = unsafe { std::slice::from_raw_parts_mut(buffer_ptr, dst_len) };
    blit_rgba_stretch(&src_rgba, sw, sh, dst, dst_w, dst_h);
    Ok(vm.ctx.none())
}

pub fn make_rasterizer_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.rasterizer", vm.ctx.new_dict(), None);
    module.set_attr("circles", vm.new_function("circles", circles), vm).unwrap();
    module.set_attr("triangles", vm.new_function("triangles", triangles_py), vm).unwrap();
    module.set_attr("lines", vm.new_function("lines", lines), vm).unwrap();
    module.set_attr("lines_batched", vm.new_function("lines_batched", lines_batched), vm).unwrap();
    module.set_attr("clear", vm.new_function("clear", clear), vm).unwrap();
    module.set_attr("fill", vm.new_function("fill", fill), vm).unwrap();
    module.set_attr("rects_filled", vm.new_function("rects_filled", rects_filled), vm).unwrap();
    module.set_attr("rectangles", vm.new_function("rectangles", rectangles), vm).unwrap();
    module.set_attr("_fill_buffer", vm.new_function("_fill_buffer", fill_buffer), vm).unwrap();
    module.set_attr("text", vm.new_function("text", text), vm).unwrap();
    module
        .set_attr("frame_in_frame", vm.new_function("frame_in_frame", frame_in_frame), vm)
        .unwrap();
    module
}
