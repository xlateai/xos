use rustpython_vm::{PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs};
use crate::python_api::rasterizer::{CURRENT_FRAME_BUFFER, CURRENT_FRAME_WIDTH, CURRENT_FRAME_HEIGHT};
use crate::ui::{Button, UiText};
use crate::ui::rich_text::{rich_text_plain_preview, rich_text_pick_char_index, rich_text_render_into_buffer};

fn py_number_to_f64(value: rustpython_vm::PyObjectRef, vm: &VirtualMachine, name: &str) -> PyResult<f64> {
    if let Ok(v) = value.clone().try_into_value::<f64>(vm) {
        return Ok(v);
    }
    if let Ok(v) = value.clone().try_into_value::<i64>(vm) {
        return Ok(v as f64);
    }
    Err(vm.new_type_error(format!(
        "{name} must be int or float"
    )))
}

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

/// Internal render hook for xos.ui.Text.render(...)
/// Usage: _text_render(text, x1, y1, x2, y2, color, hitboxes=False, baselines=False, font_size=24.0)
fn text_render(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() < 6 {
        return Err(vm.new_type_error(format!(
            "_text_render() takes at least 6 arguments ({} given)",
            args_vec.len()
        )));
    }

    let text: String = args_vec[0].clone().try_into_value(vm)?;
    let x1 = py_number_to_f64(args_vec[1].clone(), vm, "x1")?;
    let y1 = py_number_to_f64(args_vec[2].clone(), vm, "y1")?;
    let x2 = py_number_to_f64(args_vec[3].clone(), vm, "x2")?;
    let y2 = py_number_to_f64(args_vec[4].clone(), vm, "y2")?;

    let color_tuple = args_vec[5]
        .downcast_ref::<rustpython_vm::builtins::PyTuple>()
        .ok_or_else(|| vm.new_type_error("color must be a tuple".to_string()))?;
    let (r8, g8, b8, a8) = tuple_to_rgba(color_tuple, vm)?;

    let hitboxes = if args_vec.len() > 6 {
        args_vec[6].clone().try_into_value(vm)?
    } else if let Some(v) = args.kwargs.get("hitboxes") {
        v.clone().try_into_value(vm)?
    } else {
        false
    };

    let baselines = if args_vec.len() > 7 {
        args_vec[7].clone().try_into_value(vm)?
    } else if let Some(v) = args.kwargs.get("baselines") {
        v.clone().try_into_value(vm)?
    } else {
        false
    };

    let font_size_px: f32 = if args_vec.len() > 8 {
        let fs = py_number_to_f64(args_vec[8].clone(), vm, "font_size")?;
        fs as f32
    } else if let Some(v) = args.kwargs.get("font_size") {
        let fs = py_number_to_f64(v.clone(), vm, "font_size")?;
        fs as f32
    } else {
        24.0
    };

    let buffer_ptr_opt = CURRENT_FRAME_BUFFER.lock().unwrap().as_ref().map(|ptr| ptr.0);
    let canvas_width = *CURRENT_FRAME_WIDTH.lock().unwrap();
    let canvas_height = *CURRENT_FRAME_HEIGHT.lock().unwrap();

    let buffer_ptr = buffer_ptr_opt.ok_or_else(|| {
        vm.new_runtime_error("No frame buffer context set. UI must be called during tick().".to_string())
    })?;

    if !(0.0..=1.0).contains(&(x1 as f32))
        || !(0.0..=1.0).contains(&(y1 as f32))
        || !(0.0..=1.0).contains(&(x2 as f32))
        || !(0.0..=1.0).contains(&(y2 as f32))
    {
        return Err(vm.new_value_error(
            "x1, y1, x2, y2 must be normalized coordinates in [0.0, 1.0]".to_string(),
        ));
    }
    if x2 <= x1 || y2 <= y1 {
        return Err(vm.new_value_error(
            "bottom-right must be greater than top-left (x2 > x1 and y2 > y1)".to_string(),
        ));
    }

    let text_ui = UiText {
        text,
        x1_norm: x1 as f32,
        y1_norm: y1 as f32,
        x2_norm: x2 as f32,
        y2_norm: y2 as f32,
        color: (r8, g8, b8, a8),
        hitboxes,
        baselines,
        font_size_px,
    };

    let buffer = unsafe { std::slice::from_raw_parts_mut(buffer_ptr, canvas_width * canvas_height * 4) };
    let render_state = text_ui
        .render(buffer, canvas_width, canvas_height)
        .map_err(|e| vm.new_runtime_error(e))?;

    let lines_py = vm.ctx.new_list(
        render_state
            .lines
            .iter()
            .map(|v| vm.ctx.new_int(*v).into())
            .collect(),
    );
    let hitboxes_py = vm.ctx.new_list(
        render_state
            .hitboxes
            .iter()
            .map(|hb| {
                let top_left = vm.ctx.new_list(vec![
                    vm.ctx.new_float(hb[0][0] as f64).into(),
                    vm.ctx.new_float(hb[0][1] as f64).into(),
                ]);
                let bottom_right = vm.ctx.new_list(vec![
                    vm.ctx.new_float(hb[1][0] as f64).into(),
                    vm.ctx.new_float(hb[1][1] as f64).into(),
                ]);
                vm.ctx
                    .new_list(vec![top_left.into(), bottom_right.into()])
                    .into()
            })
            .collect(),
    );
    let baselines_py = vm.ctx.new_list(
        render_state
            .baselines
            .iter()
            .map(|b| {
                let p0 = vm.ctx.new_list(vec![
                    vm.ctx.new_float(b[0][0] as f64).into(),
                    vm.ctx.new_float(b[0][1] as f64).into(),
                ]);
                let p1 = vm.ctx.new_list(vec![
                    vm.ctx.new_float(b[1][0] as f64).into(),
                    vm.ctx.new_float(b[1][1] as f64).into(),
                ]);
                vm.ctx
                    .new_list(vec![p0.into(), p1.into()])
                    .into()
            })
            .collect(),
    );

    let state = vm.ctx.new_dict();
    state.set_item("lines", lines_py.into(), vm)?;
    state.set_item("hitboxes", hitboxes_py.into(), vm)?;
    state.set_item("baselines", baselines_py.into(), vm)?;
    Ok(state.into())
}

fn tuple_to_rgba(
    color_tuple: &rustpython_vm::builtins::PyTuple,
    vm: &VirtualMachine,
) -> PyResult<(u8, u8, u8, u8)> {
    let color_items = color_tuple.as_slice();
    if color_items.len() != 3 && color_items.len() != 4 {
        return Err(vm.new_type_error("color must be (r, g, b) or (r, g, b, a)".to_string()));
    }
    let r: i32 = color_items[0].clone().try_into_value(vm)?;
    let g: i32 = color_items[1].clone().try_into_value(vm)?;
    let b: i32 = color_items[2].clone().try_into_value(vm)?;
    let a: i32 = if color_items.len() == 4 {
        color_items[3].clone().try_into_value(vm)?
    } else {
        255
    };
    Ok((
        r.clamp(0, 255) as u8,
        g.clamp(0, 255) as u8,
        b.clamp(0, 255) as u8,
        a.clamp(0, 255) as u8,
    ))
}

/// Rich text: Minecraft `&` codes + `<b>`…`</b>`, rasterized like viewport `Text`.
/// `_rich_render(..., selection_start=-1, selection_end=-1)` skips selection highlight.
/// Selection indices refer to plain visible text (indices match `_rich_plain` order).
#[allow(clippy::too_many_arguments)]
fn rich_render(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() < 6 {
        return Err(vm.new_type_error(format!(
            "_rich_render() takes at least 6 arguments ({} given)",
            args_vec.len()
        )));
    }

    let raw: String = args_vec[0].clone().try_into_value(vm)?;
    let x1 = py_number_to_f64(args_vec[1].clone(), vm, "x1")?;
    let y1 = py_number_to_f64(args_vec[2].clone(), vm, "y1")?;
    let x2 = py_number_to_f64(args_vec[3].clone(), vm, "x2")?;
    let y2 = py_number_to_f64(args_vec[4].clone(), vm, "y2")?;

    let color_tuple = args_vec[5]
        .downcast_ref::<rustpython_vm::builtins::PyTuple>()
        .ok_or_else(|| vm.new_type_error("color must be a tuple".to_string()))?;
    let (r8, g8, b8, a8) = tuple_to_rgba(color_tuple, vm)?;

    let mut hitboxes = false;
    let mut baselines = false;
    let mut font_size_px: f32 = 24.0;
    let mut minecraft = true;
    let mut sel_lo: i64 = -1;
    let mut sel_hi: i64 = -1;

    if args_vec.len() > 6 {
        hitboxes = args_vec[6].clone().try_into_value(vm)?;
    } else if let Some(v) = args.kwargs.get("hitboxes") {
        hitboxes = v.clone().try_into_value(vm)?;
    }

    if args_vec.len() > 7 {
        baselines = args_vec[7].clone().try_into_value(vm)?;
    } else if let Some(v) = args.kwargs.get("baselines") {
        baselines = v.clone().try_into_value(vm)?;
    }

    if args_vec.len() > 8 {
        font_size_px = py_number_to_f64(args_vec[8].clone(), vm, "font_size")? as f32;
    } else if let Some(v) = args.kwargs.get("font_size") {
        font_size_px = py_number_to_f64(v.clone(), vm, "font_size")? as f32;
    }

    if args_vec.len() > 9 {
        minecraft = args_vec[9].clone().try_into_value(vm)?;
    } else if let Some(v) = args.kwargs.get("minecraft") {
        minecraft = v.clone().try_into_value(vm)?;
    }

    if args_vec.len() > 10 {
        sel_lo = py_number_to_f64(args_vec[10].clone(), vm, "selection_start")? as i64;
    } else if let Some(v) = args.kwargs.get("selection_start") {
        sel_lo = py_number_to_f64(v.clone(), vm, "selection_start")? as i64;
    }

    if args_vec.len() > 11 {
        sel_hi = py_number_to_f64(args_vec[11].clone(), vm, "selection_end")? as i64;
    } else if let Some(v) = args.kwargs.get("selection_end") {
        sel_hi = py_number_to_f64(v.clone(), vm, "selection_end")? as i64;
    }

    let buffer_ptr_opt = CURRENT_FRAME_BUFFER.lock().unwrap().as_ref().map(|ptr| ptr.0);
    let canvas_width = *CURRENT_FRAME_WIDTH.lock().unwrap();
    let canvas_height = *CURRENT_FRAME_HEIGHT.lock().unwrap();

    let buffer_ptr = buffer_ptr_opt.ok_or_else(|| {
        vm.new_runtime_error(
            "No frame buffer context set. UI must be called during tick().".to_string(),
        )
    })?;

    if !(0.0..=1.0).contains(&(x1 as f32))
        || !(0.0..=1.0).contains(&(y1 as f32))
        || !(0.0..=1.0).contains(&(x2 as f32))
        || !(0.0..=1.0).contains(&(y2 as f32))
    {
        return Err(vm.new_value_error(
            "x1, y1, x2, y2 must be normalized coordinates in [0.0, 1.0]".to_string(),
        ));
    }
    if x2 <= x1 || y2 <= y1 {
        return Err(vm.new_value_error(
            "bottom-right must be greater than top-left (x2 > x1 and y2 > y1)".to_string(),
        ));
    }

    let sel = if sel_lo >= 0 && sel_hi > sel_lo {
        Some((sel_lo as usize, sel_hi as usize))
    } else {
        None
    };

    let default_fg = [r8, g8, b8, a8];
    let buffer = unsafe {
        std::slice::from_raw_parts_mut(buffer_ptr, canvas_width * canvas_height * 4)
    };
    let render_state = rich_text_render_into_buffer(
        buffer,
        canvas_width,
        canvas_height,
        &raw,
        x1 as f32,
        y1 as f32,
        x2 as f32,
        y2 as f32,
        default_fg,
        font_size_px.max(1.0),
        minecraft,
        hitboxes,
        baselines,
        sel,
    )
    .map_err(|e| vm.new_runtime_error(e))?;

    let lines_py = vm.ctx.new_list(
        render_state
            .lines
            .iter()
            .map(|v| vm.ctx.new_int(*v).into())
            .collect(),
    );
    let hitboxes_py = vm.ctx.new_list(
        render_state
            .hitboxes
            .iter()
            .map(|hb| {
                let top_left = vm.ctx.new_list(vec![
                    vm.ctx.new_float(hb[0][0] as f64).into(),
                    vm.ctx.new_float(hb[0][1] as f64).into(),
                ]);
                let bottom_right = vm.ctx.new_list(vec![
                    vm.ctx.new_float(hb[1][0] as f64).into(),
                    vm.ctx.new_float(hb[1][1] as f64).into(),
                ]);
                vm.ctx
                    .new_list(vec![top_left.into(), bottom_right.into()])
                    .into()
            })
            .collect(),
    );
    let baselines_py = vm.ctx.new_list(
        render_state
            .baselines
            .iter()
            .map(|b| {
                let p0 = vm.ctx.new_list(vec![
                    vm.ctx.new_float(b[0][0] as f64).into(),
                    vm.ctx.new_float(b[0][1] as f64).into(),
                ]);
                let p1 = vm.ctx.new_list(vec![
                    vm.ctx.new_float(b[1][0] as f64).into(),
                    vm.ctx.new_float(b[1][1] as f64).into(),
                ]);
                vm.ctx
                    .new_list(vec![p0.into(), p1.into()])
                    .into()
            })
            .collect(),
    );

    let state = vm.ctx.new_dict();
    state.set_item("lines", lines_py.into(), vm)?;
    state.set_item("hitboxes", hitboxes_py.into(), vm)?;
    state.set_item("baselines", baselines_py.into(), vm)?;
    Ok(state.into())
}

fn rich_pick(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() < 8 {
        return Err(vm.new_type_error(format!(
            "_rich_pick() takes at least 8 arguments ({} given)",
            args_vec.len()
        )));
    }

    let raw: String = args_vec[0].clone().try_into_value(vm)?;
    let mx = py_number_to_f64(args_vec[1].clone(), vm, "mx")? as f32;
    let my = py_number_to_f64(args_vec[2].clone(), vm, "my")? as f32;
    let x1 = py_number_to_f64(args_vec[3].clone(), vm, "x1")?;
    let y1 = py_number_to_f64(args_vec[4].clone(), vm, "y1")?;
    let x2 = py_number_to_f64(args_vec[5].clone(), vm, "x2")?;
    let y2 = py_number_to_f64(args_vec[6].clone(), vm, "y2")?;

    let color_tuple = args_vec[7]
        .downcast_ref::<rustpython_vm::builtins::PyTuple>()
        .ok_or_else(|| vm.new_type_error("color must be a tuple".to_string()))?;
    let (r8, g8, b8, a8) = tuple_to_rgba(color_tuple, vm)?;

    let mut font_size_px: f32 = 24.0;
    let mut minecraft = true;

    if args_vec.len() > 8 {
        font_size_px = py_number_to_f64(args_vec[8].clone(), vm, "font_size")? as f32;
    } else if let Some(v) = args.kwargs.get("font_size") {
        font_size_px = py_number_to_f64(v.clone(), vm, "font_size")? as f32;
    }

    if args_vec.len() > 9 {
        minecraft = args_vec[9].clone().try_into_value(vm)?;
    } else if let Some(v) = args.kwargs.get("minecraft") {
        minecraft = v.clone().try_into_value(vm)?;
    }

    let canvas_width = *CURRENT_FRAME_WIDTH.lock().unwrap();
    let canvas_height = *CURRENT_FRAME_HEIGHT.lock().unwrap();

    if canvas_width == 0 || canvas_height == 0 {
        return Ok(vm.ctx.new_int(-1).into());
    }

    let ix = rich_text_pick_char_index(
        &raw,
        mx.round() as i32,
        my.round() as i32,
        x1 as f32,
        y1 as f32,
        x2 as f32,
        y2 as f32,
        [r8, g8, b8, a8],
        font_size_px.max(1.0),
        minecraft,
        canvas_width,
        canvas_height,
    )
    .map_err(|e| vm.new_runtime_error(e))?;

    Ok(vm.ctx.new_int(ix).into())
}

fn rich_plain(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.is_empty() {
        return Err(vm.new_type_error(
            "_rich_plain() takes at least 1 argument (text)".to_string(),
        ));
    }
    let raw: String = args_vec[0].clone().try_into_value(vm)?;

    let mut minecraft = true;
    let mut default_fg = [255u8, 255, 255, 255];

    if args_vec.len() > 1 {
        minecraft = args_vec[1].clone().try_into_value(vm)?;
    } else if let Some(v) = args.kwargs.get("minecraft") {
        minecraft = v.clone().try_into_value(vm)?;
    }

    if args_vec.len() > 2 {
        let color_tuple = args_vec[2]
            .downcast_ref::<rustpython_vm::builtins::PyTuple>()
            .ok_or_else(|| vm.new_type_error("color must be a tuple".to_string()))?;
        let rgba = tuple_to_rgba(color_tuple, vm)?;
        default_fg = [rgba.0, rgba.1, rgba.2, rgba.3];
    }

    if let Some(tuple_obj) = args.kwargs.get("color") {
        let color_tuple = tuple_obj
            .downcast_ref::<rustpython_vm::builtins::PyTuple>()
            .ok_or_else(|| vm.new_type_error("color must be a tuple".to_string()))?;
        let rgba = tuple_to_rgba(color_tuple, vm)?;
        default_fg = [rgba.0, rgba.1, rgba.2, rgba.3];
    }

    let s = rich_text_plain_preview(&raw, minecraft, default_fg);
    Ok(vm.ctx.new_str(s.as_str()).into())
}

pub fn make_ui_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.ui", vm.ctx.new_dict(), None);
    module.set_attr("button", vm.new_function("button", button), vm).unwrap();
    module.set_attr("button_contains", vm.new_function("button_contains", button_contains), vm).unwrap();
    module
        .set_attr("_text_render", vm.new_function("_text_render", text_render), vm)
        .unwrap();
    module
        .set_attr("_rich_render", vm.new_function("_rich_render", rich_render), vm)
        .unwrap();
    module
        .set_attr("_rich_pick", vm.new_function("_rich_pick", rich_pick), vm)
        .unwrap();
    module
        .set_attr("_rich_plain", vm.new_function("_rich_plain", rich_plain), vm)
        .unwrap();

    let scope = vm.new_scope_with_builtins();
    let text_render_fn = module.get_attr("_text_render", vm).unwrap();
    scope.globals.set_item("_text_render", text_render_fn, vm).unwrap();
    let rich_render_fn = module.get_attr("_rich_render", vm).unwrap();
    scope.globals.set_item("_rich_render", rich_render_fn, vm).unwrap();
    let rich_pick_fn = module.get_attr("_rich_pick", vm).unwrap();
    scope.globals.set_item("_rich_pick", rich_pick_fn, vm).unwrap();
    let rich_plain_fn = module.get_attr("_rich_plain", vm).unwrap();
    scope.globals.set_item("_rich_plain", rich_plain_fn, vm).unwrap();
    let py_text_code = r#"
def _viewport_scaled_font(font_size_px):
    """F3 / viewport UI scale (`Application.xos_scale`, percent/100) multiplies rasterized text size."""
    import builtins
    app = getattr(builtins, "__xos_app_instance__", None)
    sc = float(getattr(app, "xos_scale", 1.0)) if app is not None else 1.0
    return float(font_size_px) * sc

class Text:
    def __init__(
        self,
        text,
        x1,
        y1,
        x2,
        y2,
        color=(255, 255, 255),
        hitboxes=False,
        baselines=False,
        font_size=24.0,
        placeholder="",
        mutable=False,
        show_cursor=True,
    ):
        self.text = text
        self.x1 = x1
        self.y1 = y1
        self.x2 = x2
        self.y2 = y2
        self.color = color
        self.hitboxes = hitboxes
        self.baselines = baselines
        self.font_size = font_size
        self.placeholder = placeholder
        self.mutable = mutable
        self.show_cursor = show_cursor
        self._last_render_state = None

    def contains_pixel(self, px, py, frame_w=None, frame_h=None):
        """Normalized rect hit test vs pixel pointer (omit frame_w/h to use Application size)."""
        if frame_w is None or frame_h is None:
            import builtins

            app = getattr(builtins, "__xos_app_instance__", None)
            if app is None:
                return False
            frame_w = int(app.frame.get_width())
            frame_h = int(app.frame.get_height())
        return (
            float(self.x1) * frame_w <= float(px) < float(self.x2) * frame_w
            and float(self.y1) * frame_h <= float(py) < float(self.y2) * frame_h
        )

    def render(self, frame=None, color=None, hitboxes=None, baselines=None, font_size=None):
        import xos
        resolved_color = self.color if color is None else color
        resolved_hitboxes = self.hitboxes if hitboxes is None else hitboxes
        resolved_baselines = self.baselines if baselines is None else baselines
        resolved_font_size = _viewport_scaled_font(
            self.font_size if font_size is None else font_size
        )
        bound = False
        if frame is not None:
            fd = getattr(frame, "_data", None)
            if fd is not None and fd.get("_xos_viewport_id") is not None:
                if not xos.frame._has_context():
                    vid = int(fd["_xos_viewport_id"])
                    w = int(fd["width"])
                    h = int(fd["height"])
                    xos.frame._begin_standalone(vid, w, h)
                    bound = True
        try:
            state = _text_render(
                self.text,
                self.x1,
                self.y1,
                self.x2,
                self.y2,
                resolved_color,
                resolved_hitboxes,
                resolved_baselines,
                resolved_font_size,
            )
            wrapped = TextRenderState(state)
            self._last_render_state = wrapped
            return wrapped
        finally:
            if bound:
                xos.frame._end_standalone()

class TextRenderState:
    def __init__(self, state_dict):
        import xos
        self.lines = xos.tensor(state_dict["lines"], dtype=xos.int32)
        hb = state_dict["hitboxes"]
        bl = state_dict["baselines"]
        n_hb = len(hb)
        n_bl = len(bl)
        self.hitboxes = xos.tensor(hb, (n_hb, 2, 2), dtype=xos.float32)
        self.baselines = xos.tensor(bl, (n_bl, 2, 2), dtype=xos.float32)

def text(
    text,
    x1,
    y1,
    x2,
    y2,
    color=(255, 255, 255),
    hitboxes=False,
    baselines=False,
    font_size=24.0,
    placeholder="",
    mutable=False,
    show_cursor=True,
):
    return Text(
        text,
        x1,
        y1,
        x2,
        y2,
        color=color,
        hitboxes=hitboxes,
        baselines=baselines,
        font_size=font_size,
        placeholder=placeholder,
        mutable=mutable,
        show_cursor=show_cursor,
    )

class RichText:
    """Viewport rich text: optional Minecraft `&` color codes and `<b>`…`</b>` (bold via faux stroke).
    Selection indices are plain-text codepoint indices (see `plain()`)."""

    def __init__(
        self,
        text,
        x1,
        y1,
        x2,
        y2,
        color=(255, 255, 255),
        hitboxes=False,
        baselines=False,
        font_size=24.0,
        minecraft=True,
        selectable=False,
        editable=False,
        use_system_keyboard=False,
        placeholder="",
        mutable=False,
        show_cursor=True,
    ):
        self.text = text
        self.x1 = x1
        self.y1 = y1
        self.x2 = x2
        self.y2 = y2
        self.color = color
        self.hitboxes = hitboxes
        self.baselines = baselines
        self.font_size = font_size
        self.minecraft = minecraft
        self.selectable = selectable
        self.editable = editable
        self.use_system_keyboard = use_system_keyboard
        self.placeholder = placeholder
        self.mutable = mutable
        self.show_cursor = show_cursor
        self._last_render_state = None

    def plain(self):
        return _rich_plain(self.text, self.minecraft, color=self.color)

    def contains_pixel(self, px, py, frame_w=None, frame_h=None):
        if frame_w is None or frame_h is None:
            import builtins

            app = getattr(builtins, "__xos_app_instance__", None)
            if app is None:
                return False
            frame_w = int(app.frame.get_width())
            frame_h = int(app.frame.get_height())
        return (
            float(self.x1) * frame_w <= float(px) < float(self.x2) * frame_w
            and float(self.y1) * frame_h <= float(py) < float(self.y2) * frame_h
        )

    def pick(self, px, py):
        return int(
            _rich_pick(
                self.text,
                float(px),
                float(py),
                self.x1,
                self.y1,
                self.x2,
                self.y2,
                self.color,
                _viewport_scaled_font(self.font_size),
                self.minecraft,
            )
        )

    def render(
        self,
        frame=None,
        color=None,
        hitboxes=None,
        baselines=None,
        font_size=None,
        minecraft=None,
        selection_start=-1,
        selection_end=-1,
    ):
        import xos

        resolved_color = self.color if color is None else color
        resolved_hitboxes = self.hitboxes if hitboxes is None else hitboxes
        resolved_baselines = self.baselines if baselines is None else baselines
        resolved_font_size = _viewport_scaled_font(
            self.font_size if font_size is None else font_size
        )
        resolved_mc = self.minecraft if minecraft is None else minecraft
        bound = False
        if frame is not None:
            fd = getattr(frame, "_data", None)
            if fd is not None and fd.get("_xos_viewport_id") is not None:
                if not xos.frame._has_context():
                    vid = int(fd["_xos_viewport_id"])
                    w = int(fd["width"])
                    h = int(fd["height"])
                    xos.frame._begin_standalone(vid, w, h)
                    bound = True
        try:
            state = _rich_render(
                self.text,
                self.x1,
                self.y1,
                self.x2,
                self.y2,
                resolved_color,
                resolved_hitboxes,
                resolved_baselines,
                resolved_font_size,
                resolved_mc,
                selection_start,
                selection_end,
            )
            wrapped = TextRenderState(state)
            self._last_render_state = wrapped
            return wrapped
        finally:
            if bound:
                xos.frame._end_standalone()


def rich_text(
    text,
    x1,
    y1,
    x2,
    y2,
    color=(255, 255, 255),
    hitboxes=False,
    baselines=False,
    font_size=24.0,
    minecraft=True,
    selectable=False,
    editable=False,
    use_system_keyboard=False,
    placeholder="",
    mutable=False,
    show_cursor=True,
):
    return RichText(
        text,
        x1,
        y1,
        x2,
        y2,
        color=color,
        hitboxes=hitboxes,
        baselines=baselines,
        font_size=font_size,
        minecraft=minecraft,
        selectable=selectable,
        editable=editable,
        use_system_keyboard=use_system_keyboard,
        placeholder=placeholder,
        mutable=mutable,
        show_cursor=show_cursor,
    )
"#;
    let _ = vm.run_code_string(scope.clone(), py_text_code, "<xos_ui>".to_string());
    if let Ok(text_class) = scope.globals.get_item("Text", vm) {
        module.set_attr("Text", text_class, vm).unwrap();
    }
    if let Ok(state_class) = scope.globals.get_item("TextRenderState", vm) {
        module.set_attr("TextRenderState", state_class, vm).unwrap();
    }
    if let Ok(text_fn) = scope.globals.get_item("text", vm) {
        module.set_attr("text", text_fn, vm).unwrap();
    }
    if let Ok(c) = scope.globals.get_item("RichText", vm) {
        module.set_attr("RichText", c, vm).unwrap();
    }
    if let Ok(c) = scope.globals.get_item("rich_text", vm) {
        module.set_attr("rich_text", c, vm).unwrap();
    }

    module
}

