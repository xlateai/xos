use rustpython_vm::{PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use once_cell::sync::Lazy;

use crate::apps::text::TranscriptTextView;
use crate::python_api::rasterizer::{CURRENT_FRAME_BUFFER, CURRENT_FRAME_HEIGHT, CURRENT_FRAME_WIDTH};
use crate::rasterizer::text::fonts;
use crate::ui::{Button, UiText};
use crate::ui::rich_text::{
    blit_rgba_subrect, rich_text_plain_preview, rich_text_pick_char_index, rich_text_render_into_buffer,
    rgba_subrect_clone,
};
use crate::ui::text::UiTextRenderState;

static NEXT_SCROLL_VIEW_ID: AtomicU64 = AtomicU64::new(1);
static SCROLL_TEXT_VIEWS: Lazy<Mutex<HashMap<u64, TranscriptTextView>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Optional host-assigned slot for `_rich_render`/`RichText`: blit identical pixels across ticks (Study feedback panel, …).
#[derive(Clone, PartialEq, Eq)]
struct RichTextBlitCacheKey {
    canvas_w: usize,
    canvas_h: usize,
    raw: String,
    x1n: u32,
    y1n: u32,
    x2n: u32,
    y2n: u32,
    font_bits: u32,
    minecraft: bool,
    default_fg: [u8; 4],
    hitboxes: bool,
    baselines: bool,
    sel: Option<(usize, usize)>,
}

struct RichTextBlitCacheSlot {
    key: RichTextBlitCacheKey,
    pixels: Vec<u8>,
    bw: usize,
    bh: usize,
    px_x1: i32,
    px_y1: i32,
    lines: Vec<u32>,
    hitboxes: Vec<[[f32; 2]; 2]>,
    baselines: Vec<[[f32; 2]; 2]>,
}

static RICH_TEXT_BLIT_CACHE_SLOTS: Lazy<Mutex<HashMap<i64, RichTextBlitCacheSlot>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

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
///
/// Keyword-only: **`cache_slot=int`** caches the RGBA rectangle for identical arguments (cheap blit vs full raster).
/// Assign a stable id per logical panel (`RichText.render(..., cache_slot=1)`). Selection / text / geometry invalidate automatically.
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

    let cache_slot: Option<i64> = match args.kwargs.get("cache_slot") {
        None => None,
        Some(obj) => Some(obj.clone().try_into_value::<i64>(vm).map_err(|_| {
            vm.new_type_error("cache_slot must be int or omitted".to_string())
        })?),
    };

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
    let x1n = x1 as f32;
    let y1n = y1 as f32;
    let x2n = x2 as f32;
    let y2n = y2 as f32;
    let px1 = (x1n.clamp(0.0, 1.0) * canvas_width as f32).round() as i32;
    let py1 = (y1n.clamp(0.0, 1.0) * canvas_height as f32).round() as i32;
    let px2 = (x2n.clamp(0.0, 1.0) * canvas_width as f32).round() as i32;
    let py2 = (y2n.clamp(0.0, 1.0) * canvas_height as f32).round() as i32;

    let cache_key = RichTextBlitCacheKey {
        canvas_w: canvas_width,
        canvas_h: canvas_height,
        raw: raw.clone(),
        x1n: x1n.to_bits(),
        y1n: y1n.to_bits(),
        x2n: x2n.to_bits(),
        y2n: y2n.to_bits(),
        font_bits: font_size_px.max(1.0).to_bits(),
        minecraft,
        default_fg,
        hitboxes,
        baselines,
        sel,
    };

    let buffer = unsafe {
        std::slice::from_raw_parts_mut(buffer_ptr, canvas_width * canvas_height * 4)
    };

    let render_state = if let Some(slot) = cache_slot {
        let cached_hit = {
            let g = RICH_TEXT_BLIT_CACHE_SLOTS.lock().unwrap();
            g.get(&slot)
                .filter(|hit| hit.key == cache_key)
                .map(|hit| {
                    (
                        hit.px_x1,
                        hit.px_y1,
                        hit.bw,
                        hit.bh,
                        hit.pixels.clone(),
                        hit.lines.clone(),
                        hit.hitboxes.clone(),
                        hit.baselines.clone(),
                    )
                })
        };

        if let Some((hit_px1, hit_py1, bw, bh, pixels, lines, hitboxes, baselines)) = cached_hit
        {
            let _ = blit_rgba_subrect(buffer, canvas_width, hit_px1, hit_py1, &pixels, bw, bh);
            UiTextRenderState {
                lines,
                hitboxes,
                baselines,
            }
        } else {
            let rs = rich_text_render_into_buffer(
                buffer,
                canvas_width,
                canvas_height,
                &raw,
                x1n,
                y1n,
                x2n,
                y2n,
                default_fg,
                font_size_px.max(1.0),
                minecraft,
                hitboxes,
                baselines,
                sel,
            )
            .map_err(|e| vm.new_runtime_error(e))?;
            if px2 > px1 && py2 > py1 {
                if let Some((pixels, bw, bh)) =
                    rgba_subrect_clone(buffer, canvas_width, px1, py1, px2, py2)
                {
                    let mut cg = RICH_TEXT_BLIT_CACHE_SLOTS.lock().unwrap();
                    cg.insert(
                        slot,
                        RichTextBlitCacheSlot {
                            key: cache_key,
                            px_x1: px1,
                            px_y1: py1,
                            bw,
                            bh,
                            pixels,
                            lines: rs.lines.clone(),
                            hitboxes: rs.hitboxes.clone(),
                            baselines: rs.baselines.clone(),
                        },
                    );
                }
            }
            rs
        }
    } else {
        rich_text_render_into_buffer(
            buffer,
            canvas_width,
            canvas_height,
            &raw,
            x1n,
            y1n,
            x2n,
            y2n,
            default_fg,
            font_size_px.max(1.0),
            minecraft,
            hitboxes,
            baselines,
            sel,
        )
        .map_err(|e| vm.new_runtime_error(e))?
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

fn scrolling_create(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let fs: f32 = if !args.args.is_empty() {
        py_number_to_f64(args.args[0].clone(), vm, "font_size")? as f32
    } else if let Some(v) = args.kwargs.get("font_size") {
        py_number_to_f64(v.clone(), vm, "font_size")? as f32
    } else {
        24.0
    };
    let font = fonts::default_font();
    let view = TranscriptTextView::new(font, fs);
    let id = NEXT_SCROLL_VIEW_ID.fetch_add(1, Ordering::Relaxed);
    SCROLL_TEXT_VIEWS.lock().unwrap().insert(id, view);
    Ok(vm.ctx.new_int(id as usize).into())
}

fn scrolling_dispose(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() != 1 {
        return Err(vm.new_type_error(format!(
            "_scrolling_dispose() takes exactly 1 argument ({} given)",
            args_vec.len()
        )));
    }
    let id: u64 = args_vec[0].clone().try_into_value::<i64>(vm)? as u64;
    SCROLL_TEXT_VIEWS.lock().unwrap().remove(&id);
    Ok(vm.ctx.none())
}

fn scrolling_set_text(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() != 2 {
        return Err(vm.new_type_error(format!(
            "_scrolling_set_text() takes exactly 2 arguments ({} given)",
            args_vec.len()
        )));
    }
    let id: u64 = args_vec[0].clone().try_into_value::<i64>(vm)? as u64;
    let text: String = args_vec[1].clone().try_into_value(vm)?;
    let mut guard = SCROLL_TEXT_VIEWS.lock().unwrap();
    let view = guard
        .get_mut(&id)
        .ok_or_else(|| vm.new_value_error(format!("ScrollingText view id {} not found", id)))?;
    view.set_text(text);
    Ok(vm.ctx.none())
}

fn scrolling_set_font_size(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() != 2 {
        return Err(vm.new_type_error(format!(
            "_scrolling_set_font_size() takes exactly 2 arguments ({} given)",
            args_vec.len()
        )));
    }
    let id: u64 = args_vec[0].clone().try_into_value::<i64>(vm)? as u64;
    let fs: f64 = py_number_to_f64(args_vec[1].clone(), vm, "font_size")?;
    let mut guard = SCROLL_TEXT_VIEWS.lock().unwrap();
    let view = guard
        .get_mut(&id)
        .ok_or_else(|| vm.new_value_error(format!("ScrollingText view id {} not found", id)))?;
    view.set_font_size(fs as f32);
    Ok(vm.ctx.none())
}

fn scrolling_set_stick_to_tail(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() != 2 {
        return Err(vm.new_type_error(format!(
            "_scrolling_set_stick_to_tail() takes exactly 2 arguments ({} given)",
            args_vec.len()
        )));
    }
    let id: u64 = args_vec[0].clone().try_into_value::<i64>(vm)? as u64;
    let v: bool = args_vec[1].clone().try_into_value(vm)?;
    let mut guard = SCROLL_TEXT_VIEWS.lock().unwrap();
    let view = guard
        .get_mut(&id)
        .ok_or_else(|| vm.new_value_error(format!("ScrollingText view id {} not found", id)))?;
    view.stick_to_tail = v;
    Ok(vm.ctx.none())
}

fn scrolling_on_mouse_down(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() != 3 {
        return Err(vm.new_type_error(format!(
            "_scrolling_on_mouse_down() takes exactly 3 arguments ({} given)",
            args_vec.len()
        )));
    }
    let id: u64 = args_vec[0].clone().try_into_value::<i64>(vm)? as u64;
    let mx: f64 = py_number_to_f64(args_vec[1].clone(), vm, "mx")?;
    let my: f64 = py_number_to_f64(args_vec[2].clone(), vm, "my")?;
    let mut guard = SCROLL_TEXT_VIEWS.lock().unwrap();
    if let Some(view) = guard.get_mut(&id) {
        view.on_mouse_down(mx as f32, my as f32);
    }
    Ok(vm.ctx.none())
}

fn scrolling_on_mouse_move(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() != 4 {
        return Err(vm.new_type_error(format!(
            "_scrolling_on_mouse_move() takes exactly 4 arguments ({} given)",
            args_vec.len()
        )));
    }
    let id: u64 = args_vec[0].clone().try_into_value::<i64>(vm)? as u64;
    let mx: f64 = py_number_to_f64(args_vec[1].clone(), vm, "mx")?;
    let my: f64 = py_number_to_f64(args_vec[2].clone(), vm, "my")?;
    let kbd_vis: bool = args_vec[3].clone().try_into_value(vm)?;
    let mut guard = SCROLL_TEXT_VIEWS.lock().unwrap();
    if let Some(view) = guard.get_mut(&id) {
        view.on_mouse_move_drag(mx as f32, my as f32, kbd_vis);
    }
    Ok(vm.ctx.none())
}

fn scrolling_on_mouse_up(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() != 1 {
        return Err(vm.new_type_error(format!(
            "_scrolling_on_mouse_up() takes exactly 1 argument ({} given)",
            args_vec.len()
        )));
    }
    let id: u64 = args_vec[0].clone().try_into_value::<i64>(vm)? as u64;
    let mut guard = SCROLL_TEXT_VIEWS.lock().unwrap();
    if let Some(view) = guard.get_mut(&id) {
        view.on_mouse_up();
    }
    Ok(vm.ctx.none())
}

fn scrolling_on_scroll(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() != 2 {
        return Err(vm.new_type_error(format!(
            "_scrolling_on_scroll() takes exactly 2 arguments ({} given)",
            args_vec.len()
        )));
    }
    let id: u64 = args_vec[0].clone().try_into_value::<i64>(vm)? as u64;
    let dy: f64 = py_number_to_f64(args_vec[1].clone(), vm, "dy")?;
    let mut guard = SCROLL_TEXT_VIEWS.lock().unwrap();
    if let Some(view) = guard.get_mut(&id) {
        view.on_scroll(dy as f32);
    }
    Ok(vm.ctx.none())
}

fn scrolling_tick(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() != 8 {
        return Err(vm.new_type_error(format!(
            "_scrolling_tick() takes exactly 8 positional arguments ({} given)",
            args_vec.len()
        )));
    }
    let id: u64 = args_vec[0].clone().try_into_value::<i64>(vm)? as u64;
    let x1n = py_number_to_f64(args_vec[1].clone(), vm, "x1")? as f32;
    let y1n = py_number_to_f64(args_vec[2].clone(), vm, "y1")? as f32;
    let x2n = py_number_to_f64(args_vec[3].clone(), vm, "x2")? as f32;
    let y2n = py_number_to_f64(args_vec[4].clone(), vm, "y2")? as f32;
    let dt: f64 = py_number_to_f64(args_vec[5].clone(), vm, "dt")?;
    let osk_visible: bool = args_vec[6].clone().try_into_value(vm)?;
    let osk_trackpad: bool = args_vec[7].clone().try_into_value(vm)?;

    for (coord, label) in [
        (x1n as f64, "x1"),
        (y1n as f64, "y1"),
        (x2n as f64, "x2"),
        (y2n as f64, "y2"),
    ] {
        if !(0.0..=1.0).contains(&coord) {
            return Err(vm.new_value_error(format!("{label} must be normalized in [0.0, 1.0]")));
        }
    }
    if x2n <= x1n || y2n <= y1n {
        return Err(vm.new_value_error(
            "bottom-right must be greater than top-left (x2 > x1 and y2 > y1)".to_string(),
        ));
    }

    let buffer_ptr_opt = CURRENT_FRAME_BUFFER.lock().unwrap().as_ref().map(|ptr| ptr.0);
    let canvas_width = *CURRENT_FRAME_WIDTH.lock().unwrap();
    let canvas_height = *CURRENT_FRAME_HEIGHT.lock().unwrap();

    let buffer_ptr = buffer_ptr_opt.ok_or_else(|| {
        vm.new_runtime_error("No frame buffer context set. scrolling_text.tick() must run during Application.tick().".to_string())
    })?;

    if canvas_width == 0 || canvas_height == 0 {
        return Err(vm.new_runtime_error("canvas width/height unset".to_string()));
    }

    let cw = canvas_width as f32;
    let ch = canvas_height as f32;
    let rx0 = (x1n.clamp(0.0, 1.0) * cw).round();
    let ry0 = (y1n.clamp(0.0, 1.0) * ch).round();
    let rx1 = (x2n.clamp(0.0, 1.0) * cw).round();
    let ry1 = (y2n.clamp(0.0, 1.0) * ch).round();
    if rx1 <= rx0 || ry1 <= ry0 {
        return Err(vm.new_value_error(
            "scrolling rect pixel width/height must be positive after quantization".to_string(),
        ));
    }
    let rect = (rx0, ry0, rx1, ry1);

    let buffer_len = canvas_width.saturating_mul(canvas_height).saturating_mul(4);
    let buffer = unsafe { std::slice::from_raw_parts_mut(buffer_ptr, buffer_len) };

    let show_laser = osk_trackpad && osk_visible;
    let dt_f = dt as f32;

    let mut guard = SCROLL_TEXT_VIEWS.lock().unwrap();
    let view = guard
        .get_mut(&id)
        .ok_or_else(|| vm.new_value_error(format!("ScrollingText view id {} not found", id)))?;
    view.tick_with_viewport_frame(
        dt_f,
        buffer,
        canvas_width,
        canvas_height,
        show_laser,
        rect,
    );
    Ok(vm.ctx.none())
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
    module
        .set_attr(
            "_scrolling_create",
            vm.new_function("_scrolling_create", scrolling_create),
            vm,
        )
        .unwrap();
    module
        .set_attr(
            "_scrolling_dispose",
            vm.new_function("_scrolling_dispose", scrolling_dispose),
            vm,
        )
        .unwrap();
    module
        .set_attr(
            "_scrolling_set_text",
            vm.new_function("_scrolling_set_text", scrolling_set_text),
            vm,
        )
        .unwrap();
    module
        .set_attr(
            "_scrolling_set_font_size",
            vm.new_function("_scrolling_set_font_size", scrolling_set_font_size),
            vm,
        )
        .unwrap();
    module
        .set_attr(
            "_scrolling_set_stick_to_tail",
            vm.new_function("_scrolling_set_stick_to_tail", scrolling_set_stick_to_tail),
            vm,
        )
        .unwrap();
    module
        .set_attr(
            "_scrolling_on_mouse_down",
            vm.new_function("_scrolling_on_mouse_down", scrolling_on_mouse_down),
            vm,
        )
        .unwrap();
    module
        .set_attr(
            "_scrolling_on_mouse_move",
            vm.new_function("_scrolling_on_mouse_move", scrolling_on_mouse_move),
            vm,
        )
        .unwrap();
    module
        .set_attr(
            "_scrolling_on_mouse_up",
            vm.new_function("_scrolling_on_mouse_up", scrolling_on_mouse_up),
            vm,
        )
        .unwrap();
    module
        .set_attr(
            "_scrolling_on_scroll",
            vm.new_function("_scrolling_on_scroll", scrolling_on_scroll),
            vm,
        )
        .unwrap();
    module
        .set_attr(
            "_scrolling_tick",
            vm.new_function("_scrolling_tick", scrolling_tick),
            vm,
        )
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
    let scrolling_create_fn = module.get_attr("_scrolling_create", vm).unwrap();
    scope.globals
        .set_item("_scrolling_create", scrolling_create_fn, vm)
        .unwrap();
    let scrolling_dispose_fn = module.get_attr("_scrolling_dispose", vm).unwrap();
    scope.globals
        .set_item("_scrolling_dispose", scrolling_dispose_fn, vm)
        .unwrap();
    let scrolling_set_text_fn = module.get_attr("_scrolling_set_text", vm).unwrap();
    scope.globals
        .set_item("_scrolling_set_text", scrolling_set_text_fn, vm)
        .unwrap();
    let scrolling_set_font_size_fn = module.get_attr("_scrolling_set_font_size", vm).unwrap();
    scope.globals
        .set_item("_scrolling_set_font_size", scrolling_set_font_size_fn, vm)
        .unwrap();
    let scrolling_set_stick_fn = module.get_attr("_scrolling_set_stick_to_tail", vm).unwrap();
    scope.globals
        .set_item("_scrolling_set_stick_to_tail", scrolling_set_stick_fn, vm)
        .unwrap();
    let scrolling_on_md_fn = module.get_attr("_scrolling_on_mouse_down", vm).unwrap();
    scope.globals
        .set_item("_scrolling_on_mouse_down", scrolling_on_md_fn, vm)
        .unwrap();
    let scrolling_on_mm_fn = module.get_attr("_scrolling_on_mouse_move", vm).unwrap();
    scope.globals
        .set_item("_scrolling_on_mouse_move", scrolling_on_mm_fn, vm)
        .unwrap();
    let scrolling_on_mu_fn = module.get_attr("_scrolling_on_mouse_up", vm).unwrap();
    scope.globals
        .set_item("_scrolling_on_mouse_up", scrolling_on_mu_fn, vm)
        .unwrap();
    let scrolling_on_sc_fn = module.get_attr("_scrolling_on_scroll", vm).unwrap();
    scope.globals
        .set_item("_scrolling_on_scroll", scrolling_on_sc_fn, vm)
        .unwrap();
    let scrolling_tick_fn = module.get_attr("_scrolling_tick", vm).unwrap();
    scope.globals
        .set_item("_scrolling_tick", scrolling_tick_fn, vm)
        .unwrap();
    let py_text_code = r#"
def _viewport_scaled_font(font_size_px):
    """F3 / viewport UI scale (`Application.xos_scale`, percent/100) multiplies rasterized text size."""
    import builtins
    app = getattr(builtins, "__xos_app_instance__", None)
    sc = float(getattr(app, "xos_scale", 1.0)) if app is not None else 1.0
    return float(font_size_px) * sc

class ScrollingTextView:
    """Read-only text region using the same Rust scroll + glyph path as the Text app and transcription."""

    def __init__(self, font_size=24.0, stick_to_tail=True):
        fs = float(_viewport_scaled_font(font_size))
        self._hid = int(_scrolling_create(fs))
        self._x1 = self._y1 = self._x2 = self._y2 = 0.0
        _scrolling_set_stick_to_tail(self._hid, bool(stick_to_tail))

    def dispose(self):
        hid = getattr(self, "_hid", None)
        if hid is None:
            return
        try:
            _scrolling_dispose(hid)
        except Exception:
            pass
        self._hid = None

    def __del__(self):
        self.dispose()

    def contains_pixel(self, px, py, frame_w=None, frame_h=None):
        if frame_w is None or frame_h is None:
            import builtins

            app = getattr(builtins, "__xos_app_instance__", None)
            if app is None:
                return False
            frame_w = int(app.frame.get_width())
            frame_h = int(app.frame.get_height())
        return (
            float(self._x1) * frame_w <= float(px) < float(self._x2) * frame_w
            and float(self._y1) * frame_h <= float(py) < float(self._y2) * frame_h
        )

    def set_text(self, s):
        _scrolling_set_text(self._hid, s)

    def set_font_size(self, font_size_px):
        fs = float(_viewport_scaled_font(font_size_px))
        _scrolling_set_font_size(self._hid, fs)

    def set_stick_to_tail(self, v):
        _scrolling_set_stick_to_tail(self._hid, bool(v))

    def on_mouse_down(self, mx, my):
        _scrolling_on_mouse_down(self._hid, float(mx), float(my))

    def on_mouse_move(self, mx, my, *, keyboard_shown=False):
        _scrolling_on_mouse_move(self._hid, float(mx), float(my), bool(keyboard_shown))

    def on_mouse_up(self):
        _scrolling_on_mouse_up(self._hid)

    def on_scroll(self, dy):
        _scrolling_on_scroll(self._hid, float(dy))

    def tick(self, x1, y1, x2, y2, dt=None, *, osk_visible=None, trackpad=None):
        import builtins

        self._x1, self._y1, self._x2, self._y2 = float(x1), float(y1), float(x2), float(y2)
        app = getattr(builtins, "__xos_app_instance__", None)
        if dt is None:
            dt = float(getattr(app, "dt", 0.016)) if app is not None else 0.016
        if osk_visible is None:
            osk_visible = (
                bool(getattr(app, "onscreen_keyboard_visible", False))
                if app is not None
                else False
            )
        if trackpad is None:
            trackpad = (
                bool(getattr(app, "onscreen_trackpad_mode", False))
                if app is not None
                else False
            )
        _scrolling_tick(
            self._hid,
            self._x1,
            self._y1,
            self._x2,
            self._y2,
            float(dt),
            bool(osk_visible),
            bool(trackpad),
        )

def scrolling_text(font_size=24.0, stick_to_tail=True):
    return ScrollingTextView(font_size=font_size, stick_to_tail=stick_to_tail)

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
        cache_slot=None,
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
            kw = dict(
                selection_start=selection_start,
                selection_end=selection_end,
            )
            if cache_slot is not None:
                kw["cache_slot"] = int(cache_slot)
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
                **kw,
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
    if let Ok(c) = scope.globals.get_item("ScrollingTextView", vm) {
        module.set_attr("ScrollingTextView", c, vm).unwrap();
    }
    if let Ok(c) = scope.globals.get_item("scrolling_text", vm) {
        module.set_attr("scrolling_text", c, vm).unwrap();
    }

    module
}

