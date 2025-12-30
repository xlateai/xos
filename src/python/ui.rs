use rustpython_vm::{PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs};
use crate::python::rasterizer::{CURRENT_FRAME_BUFFER, CURRENT_FRAME_WIDTH, CURRENT_FRAME_HEIGHT};
use crate::ui::Button;

/// xos.ui.button() - create and draw a button
/// 
/// Usage: xos.ui.button(x, y, width, height, text, is_hovered, bg_color, hover_color, text_color)
/// - x: x position in pixels
/// - y: y position in pixels
/// - width: button width in pixels
/// - height: button height in pixels
/// - text: button text (currently not rendered, placeholder for future)
/// - is_hovered: whether the button is hovered
/// - bg_color: optional (r, g, b) tuple for background color
/// - hover_color: optional (r, g, b) tuple for hover color
/// - text_color: optional (r, g, b) tuple for text color
/// 
/// Returns: None (draws directly to frame buffer)
fn button(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() < 6 || args_vec.len() > 9 {
        return Err(vm.new_type_error(format!(
            "button() takes 6 to 9 arguments ({} given)",
            args_vec.len()
        )));
    }
    
    // Extract required arguments
    let x: i32 = args_vec[0].clone().try_into_value(vm)?;
    let y: i32 = args_vec[1].clone().try_into_value(vm)?;
    let width: u32 = args_vec[2].clone().try_into_value(vm)?;
    let height: u32 = args_vec[3].clone().try_into_value(vm)?;
    let text: String = args_vec[4].clone().try_into_value(vm)?;
    let is_hovered: bool = args_vec[5].clone().try_into_value(vm)?;
    
    // Extract optional color arguments
    let bg_color = if args_vec.len() > 6 {
        let color_tuple = args_vec[6].downcast_ref::<rustpython_vm::builtins::PyTuple>()
            .ok_or_else(|| vm.new_type_error("bg_color must be a tuple".to_string()))?;
        let color_vec = color_tuple.as_slice();
        if color_vec.len() != 3 {
            return Err(vm.new_type_error("bg_color must be (r, g, b)".to_string()));
        }
        let r: i32 = color_vec[0].clone().try_into_value(vm)?;
        let g: i32 = color_vec[1].clone().try_into_value(vm)?;
        let b: i32 = color_vec[2].clone().try_into_value(vm)?;
        (r as u8, g as u8, b as u8)
    } else {
        (50, 150, 50) // Default green
    };
    
    let hover_color = if args_vec.len() > 7 {
        let color_tuple = args_vec[7].downcast_ref::<rustpython_vm::builtins::PyTuple>()
            .ok_or_else(|| vm.new_type_error("hover_color must be a tuple".to_string()))?;
        let color_vec = color_tuple.as_slice();
        if color_vec.len() != 3 {
            return Err(vm.new_type_error("hover_color must be (r, g, b)".to_string()));
        }
        let r: i32 = color_vec[0].clone().try_into_value(vm)?;
        let g: i32 = color_vec[1].clone().try_into_value(vm)?;
        let b: i32 = color_vec[2].clone().try_into_value(vm)?;
        (r as u8, g as u8, b as u8)
    } else {
        (70, 170, 70) // Default light green
    };
    
    let text_color = if args_vec.len() > 8 {
        let color_tuple = args_vec[8].downcast_ref::<rustpython_vm::builtins::PyTuple>()
            .ok_or_else(|| vm.new_type_error("text_color must be a tuple".to_string()))?;
        let color_vec = color_tuple.as_slice();
        if color_vec.len() != 3 {
            return Err(vm.new_type_error("text_color must be (r, g, b)".to_string()));
        }
        let r: i32 = color_vec[0].clone().try_into_value(vm)?;
        let g: i32 = color_vec[1].clone().try_into_value(vm)?;
        let b: i32 = color_vec[2].clone().try_into_value(vm)?;
        (r as u8, g as u8, b as u8)
    } else {
        (255, 255, 255) // Default white
    };
    
    // Get the frame buffer from global context
    let buffer_ptr_opt = CURRENT_FRAME_BUFFER.lock().unwrap().as_ref().map(|ptr| ptr.0);
    let canvas_width = *CURRENT_FRAME_WIDTH.lock().unwrap();
    let canvas_height = *CURRENT_FRAME_HEIGHT.lock().unwrap();
    
    let buffer_ptr = buffer_ptr_opt.ok_or_else(|| {
        vm.new_runtime_error("No frame buffer context set. UI must be called during tick().".to_string())
    })?;
    
    // Create button and draw it
    let mut btn = Button::new(x, y, width, height, text);
    btn.bg_color = bg_color;
    btn.hover_color = hover_color;
    btn.text_color = text_color;
    
    let buffer = unsafe { std::slice::from_raw_parts_mut(buffer_ptr, canvas_width * canvas_height * 4) };
    btn.draw(buffer, canvas_width as u32, canvas_height as u32, is_hovered);
    
    Ok(vm.ctx.none())
}

/// xos.ui.button_contains() - check if a point is inside a button
/// 
/// Usage: xos.ui.button_contains(button_x, button_y, button_width, button_height, point_x, point_y)
/// Returns: bool
fn button_contains(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() != 6 {
        return Err(vm.new_type_error(format!(
            "button_contains() takes exactly 6 arguments ({} given)",
            args_vec.len()
        )));
    }
    
    let button_x: i32 = args_vec[0].clone().try_into_value(vm)?;
    let button_y: i32 = args_vec[1].clone().try_into_value(vm)?;
    let button_width: u32 = args_vec[2].clone().try_into_value(vm)?;
    let button_height: u32 = args_vec[3].clone().try_into_value(vm)?;
    let point_x: f64 = args_vec[4].clone().try_into_value(vm)?;
    let point_y: f64 = args_vec[5].clone().try_into_value(vm)?;
    
    let btn = Button::new(button_x, button_y, button_width, button_height, String::new());
    let contains = btn.contains_point(point_x as f32, point_y as f32);
    
    Ok(vm.ctx.new_bool(contains).into())
}

pub fn make_ui_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.ui", vm.ctx.new_dict(), None);
    module.set_attr("button", vm.new_function("button", button), vm).unwrap();
    module.set_attr("button_contains", vm.new_function("button_contains", button_contains), vm).unwrap();
    module
}

