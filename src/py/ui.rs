use rustpython_vm::{
    builtins::PyDict, PyObjectRef, PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs,
};
use crate::apps::text::TextApp;
use crate::rasterizer::text::fonts;
use crate::rasterizer::text::ui_markup;
use crate::python_api::engine::py_engine_tls::{with_callback_engine_state_mut, with_tick_engine_state_mut};
use crate::python_api::python_text::{
    alloc_widget_id, collect_native_text_widget_render_state, dispatch_text_widget_from_app,
    insert_widget, onscreen_keyboard_top_y_norm, paint_native_embed_text_from_engine,
    peek_editor_visual_state, peek_embed_document_string, python_embed_set_document,
    pointer_mouse_in_shown_osk_strip, sync_embed_text_norm_rect, tick_text_widget,
};
use crate::python_api::rasterizer::{CURRENT_FRAME_BUFFER, CURRENT_FRAME_HEIGHT, CURRENT_FRAME_WIDTH};
use crate::ui::{Button, UiText};

fn frame_wh_from_app(vm: &VirtualMachine, app: PyObjectRef) -> PyResult<(u32, u32)> {
    let frame = vm.get_attribute_opt(app.clone(), "frame")?.ok_or_else(|| {
        vm.new_attribute_error("Application has no 'frame' attribute".to_string())
    })?;
    let data_obj = match vm.get_attribute_opt(frame.clone(), "_data") {
        Ok(Some(d)) => d,
        Ok(None) | Err(_) => frame,
    };
    let dict = data_obj.downcast_ref::<PyDict>().ok_or_else(|| {
        vm.new_type_error("application.frame must be a Frame with dict _data".to_string())
    })?;
    let w: usize = dict.get_item("width", vm)?.clone().try_into_value(vm)?;
    let h: usize = dict.get_item("height", vm)?.clone().try_into_value(vm)?;
    Ok((w as u32, h as u32))
}

fn read_bool_prop(vm: &VirtualMachine, obj: PyObjectRef, key: &'static str, default: bool) -> PyResult<bool> {
    match vm.get_attribute_opt(obj.clone(), key)? {
        Some(v) => v.clone().try_into_value(vm),
        None => Ok(default),
    }
}

fn getattr_required(vm: &VirtualMachine, obj: PyObjectRef, name: &'static str) -> PyResult<PyObjectRef> {
    vm.get_attribute_opt(obj.clone(), name)?
        .ok_or_else(|| vm.new_attribute_error(format!("missing attribute '{}'", name)))
}

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
/// Usage: `_text_render(...)` with optional kwargs `native_widget_id`, `show_cursor`, `size` (`font_size` accepted for compatibility).
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

    let mut size_px: f32 = if args_vec.len() > 8 {
        py_number_to_f64(args_vec[8].clone(), vm, "size")? as f32
    } else if let Some(v) = args.kwargs.get("size") {
        py_number_to_f64(v.clone(), vm, "size")? as f32
    } else if let Some(v) = args.kwargs.get("font_size") {
        py_number_to_f64(v.clone(), vm, "font_size")? as f32
    } else {
        24.0
    };
    let alignment_x = if let Some(v) = args.kwargs.get("alignment_x") {
        py_number_to_f64(v.clone(), vm, "alignment_x")? as f32
    } else {
        0.0
    };
    let alignment_y = if let Some(v) = args.kwargs.get("alignment_y") {
        py_number_to_f64(v.clone(), vm, "alignment_y")? as f32
    } else {
        0.0
    };
    let spacing_x = if let Some(v) = args.kwargs.get("spacing_x") {
        py_number_to_f64(v.clone(), vm, "spacing_x")? as f32
    } else {
        1.0
    };
    let spacing_y = if let Some(v) = args.kwargs.get("spacing_y") {
        py_number_to_f64(v.clone(), vm, "spacing_y")? as f32
    } else {
        1.0
    };

    let mut text = text;
    let mut should_render = true;
    let mut show_cursor = false;
    let mut cursor_position = 0usize;

    let mut selection_start_opt: Option<usize> = None;
    let mut selection_end_opt: Option<usize> = None;
    let mut trackpad_pointer: Option<(f32, f32)> = None;
    let mut viewport_scroll_y = 0.0_f32;

    if let Some(nid_obj) = args.kwargs.get("native_widget_id") {
        let nid: u64 = nid_obj.clone().try_into_value(vm)?;
        if let Some(peek) = peek_editor_visual_state(nid) {
            text = peek.text;
            cursor_position = peek.cursor_position;
            show_cursor = peek.show_cursor;
            size_px = peek.size_px;
            viewport_scroll_y = peek.scroll_y;
            selection_start_opt = peek.selection_start;
            selection_end_opt = peek.selection_end;
            trackpad_pointer = peek.trackpad_pointer;
        }
    }
    if let Some(v) = args.kwargs.get("show_cursor") {
        show_cursor = v.clone().try_into_value(vm)?;
    }
    if let Some(v) = args.kwargs.get("cursor_position") {
        cursor_position = v.clone().try_into_value(vm)?;
    }
    if let Some(v) = args.kwargs.get("render") {
        should_render = v.clone().try_into_value(vm)?;
    }
    let active_markup_start = if let Some(v) = args.kwargs.get("active_markup_start") {
        Some(v.clone().try_into_value::<usize>(vm)?)
    } else {
        None
    };
    let active_markup_end = if let Some(v) = args.kwargs.get("active_markup_end") {
        Some(v.clone().try_into_value::<usize>(vm)?)
    } else {
        None
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

    let glyph_rgba = (
        r.clamp(0, 255) as u8,
        g.clamp(0, 255) as u8,
        b.clamp(0, 255) as u8,
        a.clamp(0, 255) as u8,
    );

    let exclusion = match (active_markup_start, active_markup_end) {
        (Some(s), Some(e)) if s <= e => Some((s, e)),
        _ => None,
    };
    let (viz_text, ui_color_spans, ui_scale_spans) =
        ui_markup::strip_inline_ui_markup_with_exclusion(&text, size_px.max(1.0), exclusion);
    let viz_cursor_position = ui_markup::map_raw_cursor_to_visual_with_exclusion(
        &text,
        size_px.max(1.0),
        exclusion,
        cursor_position,
    );

    let buffer =
        unsafe { std::slice::from_raw_parts_mut(buffer_ptr, canvas_width * canvas_height * 4) };

    let mut render_state_opt = None;
    if let Some(nid_obj) = args.kwargs.get("native_widget_id") {
        if let Ok(nid) = nid_obj.clone().try_into_value::<u64>(vm) {
            if peek_editor_visual_state(nid).is_some() {
                let cw = canvas_width;
                let ch = canvas_height;
                let xa = ((x1 as f32).clamp(0.0, 1.0) * cw as f32).round() as i32;
                let ya = ((y1 as f32).clamp(0.0, 1.0) * ch as f32).round() as i32;
                let xb = ((x2 as f32).clamp(0.0, 1.0) * cw as f32).round() as i32;
                let yb = ((y2 as f32).clamp(0.0, 1.0) * ch as f32).round() as i32;
                if should_render {
                    if let Some(true) = with_tick_engine_state_mut(|engine| {
                        paint_native_embed_text_from_engine(
                            nid,
                            engine,
                            buffer,
                            cw,
                            ch,
                            glyph_rgba,
                            show_cursor,
                        )
                    }) {
                        render_state_opt = collect_native_text_widget_render_state(
                            nid,
                            xa,
                            ya,
                            xb,
                            yb,
                            viewport_scroll_y,
                            cw,
                            ch,
                            hitboxes,
                        );
                    }
                } else {
                    render_state_opt = collect_native_text_widget_render_state(
                        nid,
                        xa,
                        ya,
                        xb,
                        yb,
                        viewport_scroll_y,
                        cw,
                        ch,
                        hitboxes,
                    );
                }
            }
        }
    }

    let render_state = if let Some(rs) = render_state_opt {
        rs
    } else {
        let text_ui = UiText {
            text: viz_text,
            x1_norm: x1 as f32,
            y1_norm: y1 as f32,
            x2_norm: x2 as f32,
            y2_norm: y2 as f32,
            color: glyph_rgba,
            hitboxes,
            baselines,
            size_px,
            show_cursor,
            cursor_position: viz_cursor_position,
            selection_start: selection_start_opt,
            selection_end: selection_end_opt,
            trackpad_pointer_px: trackpad_pointer,
            viewport_scroll_y,
            color_spans: ui_color_spans,
            scale_spans: ui_scale_spans,
            alignment: (alignment_x, alignment_y),
            spacing: (spacing_x, spacing_y),
        };
        if should_render {
            text_ui
                .render(buffer, canvas_width, canvas_height)
                .map_err(|e| vm.new_runtime_error(e))?
        } else {
            // Compute layout/render state without touching the live frame buffer.
            let mut scratch = vec![0_u8; canvas_width.saturating_mul(canvas_height).saturating_mul(4)];
            text_ui
                .render(scratch.as_mut_slice(), canvas_width, canvas_height)
                .map_err(|e| vm.new_runtime_error(e))?
        }
    };

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

/// Native [`TextApp`] registration — returns integer widget id (`_native_id`).
fn text_widget_register(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let av = args.args.as_slice();
    if av.len() != 2 {
        return Err(vm.new_type_error("_text_register requires (text_ui, app)".to_string()));
    }
    let text_py = av[0].clone();
    let app_py = av[1].clone();

    let (fw_u, fh_u) = frame_wh_from_app(vm, app_py.clone())?;

    let s: String = getattr_required(vm, text_py.clone(), "text")?
        .clone()
        .try_into_value(vm)?;
    let x1 = py_number_to_f64(getattr_required(vm, text_py.clone(), "x1")?, vm, "x1")?;
    let y1 = py_number_to_f64(getattr_required(vm, text_py.clone(), "y1")?, vm, "y1")?;
    let x2 = py_number_to_f64(getattr_required(vm, text_py.clone(), "x2")?, vm, "x2")?;
    let y2 = py_number_to_f64(getattr_required(vm, text_py.clone(), "y2")?, vm, "y2")?;

    if !(0.0..=1.0).contains(&x1) || !(0.0..=1.0).contains(&y1) || !(0.0..=1.0).contains(&x2) || !(0.0..=1.0).contains(&y2) {
        return Err(vm.new_value_error(
            "Text rect x1, y1, x2, y2 must be normalized in [0.0, 1.0]".to_string(),
        ));
    }
    if !(x2 > x1 && y2 > y1) {
        return Err(vm.new_value_error("Text rect must satisfy x2 > x1 and y2 > y1".to_string()));
    }

    let (vx, vy, vw, vh) = TextApp::rounded_norm_rect_to_px(
        x1 as f32,
        y1 as f32,
        x2 as f32,
        y2 as f32,
        fw_u.max(1) as f32,
        fh_u.max(1) as f32,
    );

    let fs_raw = if let Some(v) = vm.get_attribute_opt(text_py.clone(), "size")? {
        py_number_to_f64(v, vm, "size")?
    } else if let Some(v) = vm.get_attribute_opt(text_py.clone(), "font_size")? {
        py_number_to_f64(v, vm, "font_size")?
    } else {
        return Err(vm.new_attribute_error(
            "Text requires attribute 'size' (pixels)".to_string(),
        ));
    };
    let fs = fs_raw as f32;
    // Support both new show_* names and legacy names.
    let show_hitboxes = match vm.get_attribute_opt(text_py.clone(), "show_hitboxes")? {
        Some(v) => v.clone().try_into_value(vm)?,
        None => read_bool_prop(vm, text_py.clone(), "hitboxes", false)?,
    };
    let show_baselines = match vm.get_attribute_opt(text_py.clone(), "show_baselines")? {
        Some(v) => v.clone().try_into_value(vm)?,
        None => read_bool_prop(vm, text_py.clone(), "baselines", false)?,
    };

    let selectable = read_bool_prop(vm, text_py.clone(), "selectable", true)?;
    let scrollable = read_bool_prop(vm, text_py.clone(), "scrollable", true)?;
    let editable = read_bool_prop(vm, text_py.clone(), "editable", true)?;
    let show_cursor = read_bool_prop(vm, text_py.clone(), "show_cursor", true)?;
    let shortcuts = read_bool_prop(vm, text_py.clone(), "shortcuts", true)?;
    let copypaste = read_bool_prop(vm, text_py.clone(), "copypaste", true)?;
    let alignment_obj = getattr_required(vm, text_py.clone(), "alignment")?;
    let alignment_tuple = alignment_obj
        .downcast_ref::<rustpython_vm::builtins::PyTuple>()
        .ok_or_else(|| vm.new_type_error("Text.alignment must be a tuple (x, y)".to_string()))?;
    let alignment_items = alignment_tuple.as_slice();
    if alignment_items.len() != 2 {
        return Err(vm.new_type_error(
            "Text.alignment must have exactly 2 values: (x, y)".to_string(),
        ));
    }
    let align_x = py_number_to_f64(alignment_items[0].clone(), vm, "alignment[0]")? as f32;
    let align_y = py_number_to_f64(alignment_items[1].clone(), vm, "alignment[1]")? as f32;
    let spacing_obj = getattr_required(vm, text_py.clone(), "spacing")?;
    let spacing_tuple = spacing_obj
        .downcast_ref::<rustpython_vm::builtins::PyTuple>()
        .ok_or_else(|| vm.new_type_error("Text.spacing must be a tuple (x, y)".to_string()))?;
    let spacing_items = spacing_tuple.as_slice();
    if spacing_items.len() != 2 {
        return Err(vm.new_type_error(
            "Text.spacing must have exactly 2 values: (x, y)".to_string(),
        ));
    }
    let spacing_x = py_number_to_f64(spacing_items[0].clone(), vm, "spacing[0]")? as f32;
    let spacing_y = py_number_to_f64(spacing_items[1].clone(), vm, "spacing[1]")? as f32;

    let mut t = TextApp::new();
    t.python_viewport_norm = Some((x1 as f32, y1 as f32, x2 as f32, y2 as f32));
    t.python_viewport = Some((vx, vy, vw, vh));
    t.set_font_size(fs);
    t.read_only = !editable;
    t.set_document_text_py_ui(s);
    t.show_cursor = show_cursor;
    t.show_debug_visuals = show_hitboxes || show_baselines;
    t.py_selectable = selectable;
    t.py_scrollable = scrollable;
    t.py_allow_shortcuts = shortcuts;
    t.py_allow_copypaste = copypaste;
    t.uses_parent_ui_scale = true;
    // Draw into an already-cleared framebuffer (Python `frame.clear`); avoid full-screen black fill each tick.
    t.transparent_background = true;
    t.embed_skip_frame_present = true;
    t.embed_fast_glyph_paint = true;
    t.follow_engine_default_font = true;
    t.engine_font_family_version_seen = fonts::default_font_version();
    t.py_alignment = (align_x.clamp(0.0, 1.0), align_y.clamp(0.0, 1.0));
    t.py_spacing = (spacing_x.max(0.0), spacing_y.max(0.0));

    let id = alloc_widget_id();
    insert_widget(id, t);
    Ok(vm.ctx.new_int(id as usize).into())
}

fn text_widget_tick(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let av = args.args.as_slice();
    if av.len() < 2 || av.len() > 7 {
        return Err(vm.new_type_error(
            "_text_tick requires (native_id, size[, py_input_focused[, alignment_x[, alignment_y[, spacing_x[, spacing_y]]]]])"
                .to_string(),
        ));
    }
    let id: usize = av[0].clone().try_into_value(vm)?;
    let fs = py_number_to_f64(av[1].clone(), vm, "size")? as f32;
    let focused = av
        .get(2)
        .map(|o| o.clone().try_into_value::<bool>(vm))
        .transpose()?
        .unwrap_or(false);
    let alignment_x = av
        .get(3)
        .map(|o| py_number_to_f64(o.clone(), vm, "alignment_x").map(|v| v as f32))
        .transpose()?
        .unwrap_or(0.0);
    let alignment_y = av
        .get(4)
        .map(|o| py_number_to_f64(o.clone(), vm, "alignment_y").map(|v| v as f32))
        .transpose()?
        .unwrap_or(0.0);
    let spacing_x = av
        .get(5)
        .map(|o| py_number_to_f64(o.clone(), vm, "spacing_x").map(|v| v as f32))
        .transpose()?
        .unwrap_or(1.0);
    let spacing_y = av
        .get(6)
        .map(|o| py_number_to_f64(o.clone(), vm, "spacing_y").map(|v| v as f32))
        .transpose()?
        .unwrap_or(1.0);
    let ran = with_tick_engine_state_mut(|state| {
        tick_text_widget(
            id as u64,
            state,
            fs,
            focused,
            alignment_x,
            alignment_y,
            spacing_x,
            spacing_y,
        )
    });
    if ran.is_none() {
        return Err(vm.new_runtime_error(
            "_text_tick must run during Application.tick (engine TLS not set)".to_string(),
        ));
    }
    Ok(vm.ctx.none())
}

fn text_widget_dispatch(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let av = args.args.as_slice();
    if av.len() != 2 {
        return Err(vm.new_type_error("_text_dispatch requires (native_id, app)".to_string()));
    }
    let id: usize = av[0].clone().try_into_value(vm)?;
    let app_py = av[1].clone();
    dispatch_text_widget_from_app(vm, id as u64, app_py)?;
    Ok(vm.ctx.none())
}

fn text_peek_document(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let av = args.args.as_slice();
    if av.len() != 1 {
        return Err(vm.new_type_error(
            "_text_peek_document requires (native_id,)".to_string(),
        ));
    }
    let id: usize = av[0].clone().try_into_value(vm)?;
    match peek_embed_document_string(id as u64) {
        Some(s) => Ok(vm.ctx.new_str(s.as_str()).into()),
        None => Err(vm.new_value_error(format!(
            "_text_peek_document: unknown or non-embedded widget id {}",
            id
        ))),
    }
}

fn text_peek_cursor(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let av = args.args.as_slice();
    if av.len() != 1 {
        return Err(vm.new_type_error(
            "_text_peek_cursor requires (native_id,)".to_string(),
        ));
    }
    let id: usize = av[0].clone().try_into_value(vm)?;
    match peek_editor_visual_state(id as u64) {
        Some(peek) => Ok(vm.ctx.new_int(peek.cursor_position).into()),
        None => Err(vm.new_value_error(format!(
            "_text_peek_cursor: unknown or non-embedded widget id {}",
            id
        ))),
    }
}

fn text_set_document(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let av = args.args.as_slice();
    if av.len() < 2 || av.len() > 3 {
        return Err(vm.new_type_error(
            "_text_set_document requires (native_id, text[, cursor_end])".to_string(),
        ));
    }
    let id: usize = av[0].clone().try_into_value(vm)?;
    let s: String = av[1].clone().try_into_value(vm)?;
    let cursor_end = av
        .get(2)
        .map(|o| o.clone().try_into_value::<bool>(vm))
        .transpose()?
        .unwrap_or(true);
    let ok = python_embed_set_document(id as u64, s, cursor_end);
    if !ok {
        return Err(vm.new_value_error(format!(
            "_text_set_document: unknown or non-embedded widget id {}",
            id
        )));
    }
    Ok(vm.ctx.none())
}

fn onscreen_keyboard_tick(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let _ = args;
    let ran = with_tick_engine_state_mut(|state| {
        let shape = state.frame.shape();
        let w = shape[1] as u32;
        let h = shape[0] as u32;
        let safe = state.frame.safe_region_boundaries.clone();
        let buf = state.frame.buffer_mut();
        state
            .keyboard
            .onscreen
            .tick(buf, w, h, state.mouse.x, state.mouse.y, &safe);
    });
    if ran.is_none() {
        return Err(vm.new_runtime_error(
            "onscreen_keyboard.tick() must run inside Application.tick() (engine context required)."
                .to_string(),
        ));
    }
    Ok(vm.ctx.none())
}

/// Normalized Y of the on-screen keyboard’s top edge (`[0,1]`, same space as `Text.y1` / `Text.y2`).
fn onscreen_keyboard_top_norm(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let _ = args;
    match with_tick_engine_state_mut(|state| onscreen_keyboard_top_y_norm(state)) {
        Some(y) => Ok(vm.ctx.new_float(y as f64).into()),
        None => Err(vm.new_runtime_error(
            "_onscreen_keyboard_top_norm() must run during Application.tick() (engine TLS required)."
                .to_string(),
        )),
    }
}

/// True during `on_events` when `mouse_*` corresponds to the visible on-screen keyboard strip (no Python focus churn).
fn text_focus_skip_pointer_for_osk(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let blocked = with_callback_engine_state_mut(|s| pointer_mouse_in_shown_osk_strip(s)).unwrap_or(false);
    Ok(vm.ctx.new_bool(blocked).into())
}

fn text_widget_sync_norm_rect(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let av = args.args.as_slice();
    if av.len() != 5 {
        return Err(vm.new_type_error(
            "_text_sync_norm_rect requires (native_id, x1, y1, x2, y2) in normalized [0,1] coordinates"
                .to_string(),
        ));
    }
    let id: usize = av[0].clone().try_into_value(vm)?;
    let x1 = py_number_to_f64(av[1].clone(), vm, "x1")? as f32;
    let y1 = py_number_to_f64(av[2].clone(), vm, "y1")? as f32;
    let x2 = py_number_to_f64(av[3].clone(), vm, "x2")? as f32;
    let y2 = py_number_to_f64(av[4].clone(), vm, "y2")? as f32;

    match with_tick_engine_state_mut(|state| sync_embed_text_norm_rect(id as u64, state, x1, y1, x2, y2)) {
        None => Err(vm.new_runtime_error(
            "_text_sync_norm_rect must run during Application.tick() (engine TLS required).".to_string(),
        )),
        Some(Ok(())) => Ok(vm.ctx.none()),
        Some(Err(msg)) => Err(vm.new_value_error(msg.to_string())),
    }
}

pub fn make_ui_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.ui", vm.ctx.new_dict(), None);
    module.set_attr("button", vm.new_function("button", button), vm).unwrap();
    module.set_attr("button_contains", vm.new_function("button_contains", button_contains), vm).unwrap();
    module
        .set_attr("_text_render", vm.new_function("_text_render", text_render), vm)
        .unwrap();
    module
        .set_attr(
            "_onscreen_keyboard_tick",
            vm.new_function("_onscreen_keyboard_tick", onscreen_keyboard_tick),
            vm,
        )
        .unwrap();
    module
        .set_attr(
            "_onscreen_keyboard_top_norm",
            vm.new_function("_onscreen_keyboard_top_norm", onscreen_keyboard_top_norm),
            vm,
        )
        .unwrap();
    module
        .set_attr(
            "_text_register",
            vm.new_function("_text_register", text_widget_register),
            vm,
        )
        .unwrap();
    module
        .set_attr(
            "_text_skip_focus_for_osk_pointer",
            vm.new_function("_text_skip_focus_for_osk_pointer", text_focus_skip_pointer_for_osk),
            vm,
        )
        .unwrap();
    module
        .set_attr(
            "_text_sync_norm_rect",
            vm.new_function("_text_sync_norm_rect", text_widget_sync_norm_rect),
            vm,
        )
        .unwrap();
    module
        .set_attr("_text_tick", vm.new_function("_text_tick", text_widget_tick), vm)
        .unwrap();
    module
        .set_attr("_text_dispatch", vm.new_function("_text_dispatch", text_widget_dispatch), vm)
        .unwrap();
    module
        .set_attr(
            "_text_peek_document",
            vm.new_function("_text_peek_document", text_peek_document),
            vm,
        )
        .unwrap();
    module
        .set_attr(
            "_text_peek_cursor",
            vm.new_function("_text_peek_cursor", text_peek_cursor),
            vm,
        )
        .unwrap();
    module
        .set_attr(
            "_text_set_document",
            vm.new_function("_text_set_document", text_set_document),
            vm,
        )
        .unwrap();

    let scope = vm.new_scope_with_builtins();
    let text_render_fn = module.get_attr("_text_render", vm).unwrap();
    scope.globals.set_item("_text_render", text_render_fn, vm).unwrap();
    let py_text_code = r#"
class OnScreenKeyboard:
    def __init__(self):
        # Normalized Y of keyboard top (`[0,1]`), same as `Text.y1`/`y2`. Updated each `tick` after OSK layout.
        self.y1 = 1.0

    def tick(self, app):
        import xos
        xos.ui._onscreen_keyboard_tick()
        try:
            self.y1 = float(xos.ui._onscreen_keyboard_top_norm())
        except RuntimeError:
            pass

    def on_events(self, app):
        pass

class Text:
    def __init__(
        self,
        text="",
        x1=0.0,
        y1=0.0,
        x2=1.0,
        y2=1.0,
        color=(255, 255, 255),
        show_hitboxes=False,
        show_baselines=False,
        size=24.0,
        font=None,
        **kwargs,
    ):
        if font is not None:
            raise TypeError(
                "xos.ui.Text does not support custom fonts yet; omit font or pass None to use the F3 / engine default."
            )
        lv = kwargs.pop("font_size", None)
        if lv is not None:
            size = lv
        lv = kwargs.pop("fontsize", None)
        if lv is not None:
            size = lv
        self.text = text
        self.x1 = x1
        self.y1 = y1
        self.x2 = x2
        self.y2 = y2
        self.color = color
        legacy_hitboxes = kwargs.get("hitboxes", False)
        legacy_baselines = kwargs.get("baselines", False)
        self.show_hitboxes = bool(kwargs.get("show_hitboxes", show_hitboxes) or legacy_hitboxes)
        self.show_baselines = bool(kwargs.get("show_baselines", show_baselines) or legacy_baselines)
        # Backward-compatible aliases.
        self.hitboxes = self.show_hitboxes
        self.baselines = self.show_baselines
        self.size = float(size)
        self._native_id = None
        self._last_tick_state = None
        self._active_markup_range = None
        self._show_markup_source = True
        self._kwargs = kwargs
        self.selectable = kwargs.get("selectable", True)
        self.scrollable = kwargs.get("scrollable", True)
        self.editable = kwargs.get("editable", True)
        self.show_cursor = kwargs.get("show_cursor", True)
        self.shortcuts = kwargs.get("shortcuts", True)
        self.copypaste = kwargs.get("copypaste", True)
        raw_alignment = kwargs.get("alignment", (0.0, 0.0))
        if isinstance(raw_alignment, (tuple, list)) and len(raw_alignment) >= 2:
            self.alignment = (float(raw_alignment[0]), float(raw_alignment[1]))
        else:
            self.alignment = (0.0, 0.0)
        raw_spacing = kwargs.get("spacing", (1.0, 1.0))
        if isinstance(raw_spacing, (tuple, list)) and len(raw_spacing) >= 2:
            self.spacing = (max(0.0, float(raw_spacing[0])), max(0.0, float(raw_spacing[1])))
        else:
            self.spacing = (1.0, 1.0)
        self.is_focused = False
        # Sticky keyboard / pointer focus: ``.focused = True`` keeps this editor receiving keys
        # even after clicks on other panes (until ``.focused = False``).
        sf = kwargs.pop("sticky_focus", False)
        fd = kwargs.pop("focused", False)
        self._sticky_focus = bool(sf or fd)
        if self._sticky_focus:
            self.is_focused = True

    def _effective_input_focus(self):
        return bool(getattr(self, "_sticky_focus", False) or self.is_focused)

    @staticmethod
    def _markup_ranges(s):
        # Returns tuples: (open[,], close], open(, close), inner_start, inner_end)
        text = str(s)
        out = []
        n = len(text)
        i = 0
        while i < n:
            if text[i] != "[":
                i += 1
                continue
            rb = text.find("](", i + 1)
            if rb == -1:
                i += 1
                continue
            rp = text.find(")", rb + 2)
            if rp == -1:
                i += 1
                continue
            out.append((i, rb, rb + 1, rp, i + 1, rb))
            i = rp + 1
        return out

    @classmethod
    def _should_show_markup_source(cls, text, cursor_pos):
        # Show source while editing near a markup token, or when label is empty.
        ranges = cls._markup_ranges(text)
        if not ranges:
            return False
        cp = int(max(0, cursor_pos))
        for lb, rb, lp, rp, inner_start, inner_end in ranges:
            if inner_start >= inner_end:
                return True
            if (lb - 1) <= cp <= (rp + 1):
                return True
        return False

    @property
    def focused(self):
        return self._effective_input_focus()

    @focused.setter
    def focused(self, value):
        v = bool(value)
        self._sticky_focus = v
        self.is_focused = v

    @property
    def font_size(self):
        return float(self.size)

    @font_size.setter
    def font_size(self, value):
        self.size = float(value)

    def tick(self, app):
        import xos
        if self._native_id is None:
            self._native_id = int(xos.ui._text_register(self, app))
        nid = int(self._native_id)
        eff = self._effective_input_focus()
        peek_s = ""
        try:
            peek_s = str(xos.ui._text_peek_document(nid))
        except (ValueError, RuntimeError, OSError):
            peek_s = ""
        if self.editable:
            # Allow fast app-driven swaps for editable regions (e.g. verdict/prompt text):
            # if Python-side text changed while this widget is not actively focused, push it
            # into the native editor before collecting render state.
            if (not eff) and (str(self.text) != peek_s):
                try:
                    xos.ui._text_set_document(nid, str(self.text), True)
                    peek_s = str(self.text)
                except (ValueError, RuntimeError, OSError):
                    pass
        else:
            if self.text != peek_s:
                try:
                    xos.ui._text_set_document(nid, str(self.text), True)
                except (ValueError, RuntimeError, OSError):
                    pass
        xos.ui._text_sync_norm_rect(
            nid,
            float(self.x1),
            float(self.y1),
            float(self.x2),
            float(self.y2),
        )
        caret = bool(self.show_cursor and eff)
        xos.ui._text_tick(
            nid,
            float(self.size),
            bool(eff),
            float(self.alignment[0]),
            float(self.alignment[1]),
            float(self.spacing[0]),
            float(self.spacing[1]),
        )
        # Keep tick-time state collection on the native path for performance.
        # Styled fallback (for unfocused editable markup preview) is handled in render().
        extra = {
            "native_widget_id": int(self._native_id),
            "show_cursor": caret,
        }
        state = xos.ui._text_render(
            self.text,
            self.x1,
            self.y1,
            self.x2,
            self.y2,
            self.color,
            # Always compute and return hitboxes/baselines in render state.
            # Visibility is controlled separately via show_* flags.
            hitboxes=True,
            baselines=True,
            size=self.size,
            alignment_x=float(self.alignment[0]),
            alignment_y=float(self.alignment[1]),
            spacing_x=float(self.spacing[0]),
            spacing_y=float(self.spacing[1]),
            render=False,
            **extra,
        )
        self._last_tick_state = TextRenderState(state)
        if self.editable:
            try:
                self.text = str(xos.ui._text_peek_document(nid))
            except (ValueError, RuntimeError, OSError):
                pass
        # Focused editable text: only show raw markup source when cursor is in/near a token,
        # or when a token has an empty [] label (so it stays editable).
        self._show_markup_source = True
        if self.editable:
            if eff:
                try:
                    cp = int(xos.ui._text_peek_cursor(nid))
                except (ValueError, RuntimeError, OSError):
                    cp = 0
                active = None
                for r in self._markup_ranges(self.text):
                    lb, _rb, _lp, rp, inner_start, inner_end = r
                    if inner_start >= inner_end or ((lb - 1) <= cp <= (rp + 1)):
                        active = r
                        break
                self._active_markup_range = active
                self._show_markup_source = active is not None
            else:
                self._active_markup_range = None
                self._show_markup_source = False
        return self._last_tick_state

    def on_events(self, app):
        import xos
        ev = getattr(app, "_xos_event", None)
        if isinstance(ev, dict) and ev.get("kind") == "mouse_down":
            # OSK taps share screen X with text columns; don't move focus — same band as [`TextApp::on_mouse_down`].
            if not xos.ui._text_skip_focus_for_osk_pointer():
                fd = getattr(app.frame, "_data", app.frame)
                fw = float(fd["width"])
                fh = float(fd["height"])
                xa = int(round(min(1.0, max(0.0, float(self.x1))) * fw))
                ya = int(round(min(1.0, max(0.0, float(self.y1))) * fh))
                xb = int(round(min(1.0, max(0.0, float(self.x2))) * fw))
                yb = int(round(min(1.0, max(0.0, float(self.y2))) * fh))
                vw = max(1, xb - xa)
                vh = max(1, yb - ya)
                # Prefer routed event coordinates (same frame as native hit-testing); fall back to app.mouse.
                mx = float(ev["x"]) if "x" in ev else float(app.mouse["x"])
                my = float(ev["y"]) if "y" in ev else float(app.mouse["y"])
                hit = xa <= mx < xa + vw and ya <= my < ya + vh
                if getattr(self, "_sticky_focus", False):
                    self.is_focused = True
                else:
                    self.is_focused = hit
        nid = getattr(self, "_native_id", None)
        if nid is None:
            self._native_id = int(xos.ui._text_register(self, app))
            nid = self._native_id
        xos.ui._text_dispatch(int(nid), app)

    def render(self, frame=None, color=None, hitboxes=None, baselines=None, size=None, font_size=None):
        import xos
        resolved_color = self.color if color is None else color
        resolved_hitboxes = self.show_hitboxes if hitboxes is None else hitboxes
        resolved_baselines = self.show_baselines if baselines is None else baselines
        if font_size is not None:
            resolved_size = float(font_size)
        elif size is not None:
            resolved_size = float(size)
        else:
            resolved_size = float(self.size)
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
            extra = {}
            nid = getattr(self, "_native_id", None)
            eff = self._effective_input_focus()
            # Editable widgets always render via markup parser path so non-active tokens remain styled.
            # Non-editable keeps native fast path.
            use_native_visual = bool(not self.editable)
            text_for_render = self.text
            if nid is not None and use_native_visual:
                extra["native_widget_id"] = int(nid)
                extra["show_cursor"] = bool(self.show_cursor and eff)
            elif eff:
                # Fallback renderer: preserve caret visibility while editing.
                try:
                    extra["cursor_position"] = int(xos.ui._text_peek_cursor(int(nid))) if nid is not None else 0
                except (ValueError, RuntimeError, OSError):
                    extra["cursor_position"] = 0
                extra["show_cursor"] = bool(self.show_cursor and eff)
            if self.editable and eff and self._active_markup_range is not None:
                lb, _rb, _lp, rp, _is, _ie = self._active_markup_range
                extra["active_markup_start"] = int(lb)
                extra["active_markup_end"] = int(rp)
            state = _text_render(
                text_for_render,
                self.x1,
                self.y1,
                self.x2,
                self.y2,
                resolved_color,
                # Always compute and return full render geometry.
                bool(resolved_hitboxes),
                bool(resolved_baselines),
                resolved_size,
                alignment_x=float(self.alignment[0]),
                alignment_y=float(self.alignment[1]),
                spacing_x=float(self.spacing[0]),
                spacing_y=float(self.spacing[1]),
                **extra,
            )
            rendered_state = TextRenderState(state)
            self._last_tick_state = rendered_state
            return rendered_state
        finally:
            if bound:
                xos.frame._end_standalone()

class Group:
    """Sequential widget container: forwards tick() / on_events() to children (e.g. several Text editors)."""

    __slots__ = ("_children",)

    def __init__(self, *children):
        self._children = tuple(children)

    @property
    def size(self):
        cs = self._children
        if not cs:
            return 24.0
        c0 = cs[0]
        return float(getattr(c0, "size", getattr(c0, "font_size", 24.0)))

    @size.setter
    def size(self, value):
        v = float(value)
        for c in self._children:
            if hasattr(c, "size"):
                c.size = v
            elif hasattr(c, "font_size"):
                c.font_size = v

    @property
    def font_size(self):
        return float(self.size)

    @font_size.setter
    def font_size(self, value):
        self.size = float(value)

    def tick(self, app):
        # Preserve each child's return object in-order instead of
        # collapsing into a single vectorized TextRenderState.
        return tuple(c.tick(app) for c in self._children)

    def on_events(self, app):
        for c in self._children:
            if hasattr(c, "on_events"):
                c.on_events(app)

    def render(self, app=None):
        out = []
        for c in self._children:
            if hasattr(c, "render"):
                out.append(c.render())
        return tuple(out)


def group(*children):
    return Group(*children)

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

def text(text="", x1=0.0, y1=0.0, x2=1.0, y2=1.0, color=(255, 255, 255), show_hitboxes=False, show_baselines=False, size=24.0, font=None, **kwargs):
    if "font_size" in kwargs:
        size = kwargs.pop("font_size")
    if "fontsize" in kwargs:
        size = kwargs.pop("fontsize")
    return Text(
        text,
        x1=x1,
        y1=y1,
        x2=x2,
        y2=y2,
        color=color,
        show_hitboxes=show_hitboxes,
        show_baselines=show_baselines,
        size=size,
        font=font,
        **kwargs
    )

def onscreen_keyboard():
    return OnScreenKeyboard()
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
    if let Ok(kb_cls) = scope.globals.get_item("OnScreenKeyboard", vm) {
        module.set_attr("OnScreenKeyboard", kb_cls, vm).unwrap();
    }
    if let Ok(kb_fn) = scope.globals.get_item("onscreen_keyboard", vm) {
        module.set_attr("onscreen_keyboard", kb_fn, vm).unwrap();
    }
    if let Ok(grp_cls) = scope.globals.get_item("Group", vm) {
        module.set_attr("Group", grp_cls, vm).unwrap();
    }
    if let Ok(grp_fn) = scope.globals.get_item("group", vm) {
        module.set_attr("group", grp_fn, vm).unwrap();
    }

    module
}

