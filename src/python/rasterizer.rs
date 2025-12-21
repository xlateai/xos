use rustpython_vm::{PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs};
use std::sync::Mutex;

// Thread-safe wrapper for raw pointer
struct FrameBufferPtr(*mut u8);
unsafe impl Send for FrameBufferPtr {}
unsafe impl Sync for FrameBufferPtr {}

// Global pointer to the current frame buffer (set during tick)
static CURRENT_FRAME_BUFFER: Mutex<Option<FrameBufferPtr>> = Mutex::new(None);
static CURRENT_FRAME_WIDTH: Mutex<usize> = Mutex::new(0);
static CURRENT_FRAME_HEIGHT: Mutex<usize> = Mutex::new(0);

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

pub fn make_rasterizer_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.rasterizer", vm.ctx.new_dict(), None);
    module.set_attr("circles", vm.new_function("circles", circles), vm).unwrap();
    module
}
