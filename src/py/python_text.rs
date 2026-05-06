//! Native backing for [`xos.ui.Text`]: registry of [`TextApp`] editors + event routing.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use rustpython_vm::builtins::PyDict;
use rustpython_vm::{PyObjectRef, PyResult, VirtualMachine};

use crate::apps::text::TextApp;
use crate::engine::keyboard::shortcuts::ShortcutAction;
use crate::engine::{Application, EngineState, ScrollWheelUnit};
use crate::ui::text::{collect_ui_text_render_state, UiTextRenderState};

/// Monotonic id assignment for [`Text`] handles (Python-visible as `_native_id`).
static NEXT_WIDGET_ID: AtomicU64 = AtomicU64::new(1);

static REGISTRY: Mutex<Option<HashMap<u64, TextApp>>> = Mutex::new(None);
/// Active embedded text widget that owns the current pointer gesture (mouse/touch drag).
/// Set on `MouseDown`, released on `MouseUp`.
static ACTIVE_POINTER_WIDGET_ID: Mutex<Option<u64>> = Mutex::new(None);

fn registry_mut() -> std::sync::MutexGuard<'static, Option<HashMap<u64, TextApp>>> {
    REGISTRY.lock().expect("python Text registry mutex poisoned")
}

fn ensure_registry<'a>(
    g: &'a mut std::sync::MutexGuard<'static, Option<HashMap<u64, TextApp>>>,
) -> &'a mut HashMap<u64, TextApp> {
    if g.is_none() {
        **g = Some(HashMap::new());
    }
    g.as_mut().unwrap()
}

pub fn alloc_widget_id() -> u64 {
    NEXT_WIDGET_ID.fetch_add(1, Ordering::Relaxed)
}

pub fn insert_widget(id: u64, editor: TextApp) {
    let mut g = registry_mut();
    ensure_registry(&mut g).insert(id, editor);
}

/// Blits a registered [`TextApp`] using layout from `_text_tick` (no second [`TextRasterizer::tick`]).
pub(crate) fn paint_native_embed_text_from_engine(
    id: u64,
    engine: &EngineState,
    buffer: &mut [u8],
    stride_w: usize,
    stride_h: usize,
    glyph_rgba: (u8, u8, u8, u8),
    paint_cursor: bool,
) -> bool {
    let mut g = registry_mut();
    let Some(map) = g.as_mut() else {
        return false;
    };
    let Some(app) = map.get_mut(&id) else {
        return false;
    };
    app.paint_into_buffer_for_engine_frame(
        engine,
        buffer,
        stride_w,
        stride_h,
        glyph_rgba,
        paint_cursor,
    )
    .is_ok()
}

pub(crate) fn collect_native_text_widget_render_state(
    id: u64,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    scroll_y: f32,
    fw: usize,
    fh: usize,
    include_hitboxes: bool,
) -> Option<UiTextRenderState> {
    let g = registry_mut();
    let map = g.as_ref()?;
    let app = map.get(&id)?;
    Some(collect_ui_text_render_state(
        &app.text_rasterizer,
        x1,
        y1,
        x2,
        y2,
        scroll_y,
        fw,
        fh,
        include_hitboxes,
    ))
}

/// Snapshot native [`TextApp`] layout, caret, selection, and trackpad laser for [`xos.ui._text_render`].
#[derive(Clone, Debug)]
pub struct EditorRenderPeek {
    pub text: String,
    pub cursor_position: usize,
    pub show_cursor: bool,
    pub size_px: f32,
    /// Vertical document scroll (must match `_text_render` layout offset).
    pub scroll_y: f32,
    pub selection_start: Option<usize>,
    pub selection_end: Option<usize>,
    /// Full-frame pixels when trackpad laser is active (same coordinates as standalone [`TextApp`] draw).
    pub trackpad_pointer: Option<(f32, f32)>,
}

pub fn peek_editor_visual_state(id: u64) -> Option<EditorRenderPeek> {
    let g = registry_mut();
    let map = g.as_ref()?;
    let t = map.get(&id)?;
    let (selection_start, selection_end, trackpad_pointer) = t.ui_peek_overlay();
    Some(EditorRenderPeek {
        text: t.text_rasterizer.text.clone(),
        cursor_position: t.cursor_position,
        show_cursor: t.show_cursor,
        size_px: t.text_rasterizer.font_size,
        scroll_y: t.scroll_y,
        selection_start,
        selection_end,
        trackpad_pointer,
    })
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum PyUiEventKind {
    MouseDown,
    MouseUp,
    MouseMove,
    Scroll {
        dx: f32,
        dy: f32,
        unit: ScrollWheelUnit,
    },
    Key(char),
    Shortcut(ShortcutAction),
}

pub(crate) fn parse_app_xos_event(vm: &VirtualMachine, app: &PyObjectRef) -> PyResult<Option<PyUiEventKind>> {
    let ev = match vm.get_attribute_opt(app.clone(), "_xos_event") {
        Ok(Some(o)) => o,
        Ok(None) => return Ok(None),
        Err(e) => return Err(e),
    };
    if vm.is_none(&ev) {
        return Ok(None);
    }
    let dict = ev
        .downcast_ref::<PyDict>()
        .ok_or_else(|| vm.new_type_error("_xos_event must be a dict".to_string()))?;
    let kind = dict.get_item("kind", vm)?.str(vm)?.to_string();
    Ok(Some(match kind.as_str() {
        "mouse_down" => PyUiEventKind::MouseDown,
        "mouse_up" => PyUiEventKind::MouseUp,
        "mouse_move" => PyUiEventKind::MouseMove,
        "scroll" => {
            let dx = dict
                .get_item("dx", vm)
                .ok()
                .and_then(|o| o.clone().try_into_value::<f64>(vm).ok())
                .unwrap_or(0.0) as f32;
            let dy = dict
                .get_item("dy", vm)
                .ok()
                .and_then(|o| o.clone().try_into_value::<f64>(vm).ok())
                .unwrap_or(0.0) as f32;
            let unit = dict
                .get_item("unit", vm)
                .ok()
                .and_then(|o| o.str(vm).ok())
                .map(|s| s.to_string())
                .map(|u| match u.to_ascii_lowercase().as_str() {
                    "line" | "lines" => ScrollWheelUnit::Line,
                    _ => ScrollWheelUnit::Pixel,
                })
                .unwrap_or(ScrollWheelUnit::Pixel);
            PyUiEventKind::Scroll { dx, dy, unit }
        }
        "key_char" => {
            let s = dict.get_item("char", vm)?.str(vm)?.to_string();
            let ch = s.chars().next().ok_or_else(|| {
                vm.new_value_error("_xos_event char must be a non-empty string".to_string())
            })?;
            if s.chars().count() != 1 {
                return Err(vm.new_value_error("_xos_event char must be a single unicode character".to_string()));
            }
            PyUiEventKind::Key(ch)
        }
        "shortcut" => {
            let a = dict.get_item("action", vm)?.str(vm)?.to_string();
            let action = match a.to_ascii_lowercase().as_str() {
                "copy" => ShortcutAction::Copy,
                "cut" => ShortcutAction::Cut,
                "paste" => ShortcutAction::Paste,
                "select_all" => ShortcutAction::SelectAll,
                "undo" => ShortcutAction::Undo,
                "redo" => ShortcutAction::Redo,
                _ => {
                    return Err(vm.new_value_error(format!(
                        "unknown shortcut action '{}' (expect copy | cut | paste | select_all | undo | redo)",
                        a
                    )));
                }
            };
            PyUiEventKind::Shortcut(action)
        }
        _ => return Ok(None),
    }))
}

pub fn dispatch_text_widget_from_app(vm: &VirtualMachine, widget_id: u64, app: PyObjectRef) -> PyResult<()> {
    let Some(kind) = parse_app_xos_event(vm, &app)? else {
        return Ok(());
    };
    crate::python_api::engine::py_engine_tls::with_callback_engine_state_mut(|state| {
        dispatch_text_widget(widget_id, kind, state);
    });
    Ok(())
}

pub fn dispatch_text_widget(id: u64, kind: PyUiEventKind, state: &mut EngineState) {
    let captured_id = ACTIVE_POINTER_WIDGET_ID
        .lock()
        .ok()
        .and_then(|g| *g);
    // Pointer gesture capture: once a widget receives mouse down, subsequent move/scroll/up
    // must route only to that same widget until release.
    match kind {
        PyUiEventKind::MouseMove | PyUiEventKind::MouseUp | PyUiEventKind::Scroll { .. } => {
            if let Some(owner) = captured_id {
                if owner != id {
                    return;
                }
            }
        }
        _ => {}
    }

    let mut g = registry_mut();
    let Some(map) = g.as_mut() else {
        return;
    };
    let Some(t) = map.get_mut(&id) else {
        return;
    };
    let mx = state.mouse.x;
    let my = state.mouse.y;
    let ptr_in_osk = pointer_mouse_in_shown_osk_strip(state);
    let trackpad_global_pointer = state.keyboard.onscreen.is_trackpad_mode() && state.keyboard.onscreen.is_shown();

    if t.python_viewport.is_some() {
        let skip = match &kind {
            PyUiEventKind::Key(_) | PyUiEventKind::Shortcut(_) => !t.py_input_focused,
            PyUiEventKind::Scroll { .. } => {
                if captured_id == Some(id) {
                    ptr_in_osk
                } else {
                    ptr_in_osk || !t.python_viewport_contains_screen_point(mx, my)
                }
            }
            // MouseDown/MouseUp: content hits must use viewport routing so an unfocused pane can take focus
            // even while the OSK is in trackpad mode (otherwise only the focused editor received clicks).
            PyUiEventKind::MouseDown | PyUiEventKind::MouseUp => {
                if ptr_in_osk && !(trackpad_global_pointer && t.py_input_focused) {
                    true
                } else if trackpad_global_pointer && ptr_in_osk {
                    !t.py_input_focused
                } else if captured_id == Some(id) {
                    false
                } else {
                    !t.python_viewport_contains_screen_point(mx, my)
                }
            }
            // MouseMove: keep focused-only delivery in trackpad mode so the laser tracks across the full
            // frame without requiring the pointer to stay inside the editor rect.
            PyUiEventKind::MouseMove => {
                if ptr_in_osk && !(trackpad_global_pointer && t.py_input_focused) {
                    true
                } else if trackpad_global_pointer {
                    !t.py_input_focused
                } else if captured_id == Some(id) {
                    false
                } else {
                    !t.python_viewport_contains_screen_point(mx, my)
                }
            }
        };
        if skip {
            return;
        }
    }
    match kind {
        PyUiEventKind::MouseDown => {
            t.on_mouse_down(state);
            if t.python_viewport.is_some() {
                if let Ok(mut cap) = ACTIVE_POINTER_WIDGET_ID.lock() {
                    *cap = Some(id);
                }
            }
        }
        PyUiEventKind::MouseUp => {
            t.on_mouse_up(state);
            if let Ok(mut cap) = ACTIVE_POINTER_WIDGET_ID.lock() {
                if *cap == Some(id) {
                    *cap = None;
                }
            }
        }
        PyUiEventKind::MouseMove => t.on_mouse_move(state),
        PyUiEventKind::Scroll { dx, dy, unit } => t.on_scroll(state, dx, dy, unit),
        PyUiEventKind::Key(ch) => t.on_key_char(state, ch),
        PyUiEventKind::Shortcut(sa) => t.apply_keyboard_shortcut(sa, state),
    }
}

pub fn tick_text_widget(
    id: u64,
    state: &mut EngineState,
    size_px: f32,
    py_input_focused: bool,
    py_alignment_x: f32,
    py_alignment_y: f32,
    py_spacing_x: f32,
    py_spacing_y: f32,
) {
    let mut g = registry_mut();
    let Some(map) = g.as_mut() else {
        return;
    };
    let Some(t) = map.get_mut(&id) else {
        return;
    };
    t.py_input_focused = py_input_focused;
    t.py_alignment = (py_alignment_x.clamp(0.0, 1.0), py_alignment_y.clamp(0.0, 1.0));
    t.py_spacing = (py_spacing_x.max(0.0), py_spacing_y.max(0.0));

    // Unfocused embed widgets must not retain a phantom trackpad laser (only the focused pane drives it).
    if t.python_viewport.is_some() && !py_input_focused {
        t.clear_trackpad_state_for_python_embed_handoff();
    }

    // Quarter-pixel quantization: stray float noise from Python shouldn't rebuild layout / clear glyphs every tick.
    let fs = (size_px * 4.0).round() / 4.0;
    if (t.text_rasterizer.font_size - fs).abs() >= 0.02 {
        t.set_font_size(fs);
    }
    t.tick(state);
}

/// Copy Python [`xos.ui.Text`] `x1..y2` into the native [`TextApp`] before [`tick_text_widget`].
/// Call every frame if coordinates may change (matches `_text_render` / hit-testing).
pub fn sync_embed_text_norm_rect(
    id: u64,
    state: &EngineState,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
) -> Result<(), &'static str> {
    if !(0.0..=1.0).contains(&x1)
        || !(0.0..=1.0).contains(&y1)
        || !(0.0..=1.0).contains(&x2)
        || !(0.0..=1.0).contains(&y2)
    {
        return Err("text rect coordinates must lie in [0.0, 1.0]");
    }
    if !(x2 > x1 && y2 > y1) {
        return Err("text rect must satisfy x2 > x1 and y2 > y1");
    }

    let shape = state.frame.shape();
    let fw = shape[1].max(1) as f32;
    let fh = shape[0].max(1) as f32;

    let mut g = registry_mut();
    let Some(map) = g.as_mut() else {
        return Err("text widget registry unavailable");
    };
    let Some(t) = map.get_mut(&id) else {
        return Err("unknown native text widget id");
    };
    if t.python_viewport_norm.is_none() {
        return Err("widget was not registered as Python embedded text");
    }

    t.python_viewport_norm = Some((x1, y1, x2, y2));
    let px = TextApp::rounded_norm_rect_to_px(x1, y1, x2, y2, fw, fh);
    t.python_viewport = Some(px);
    Ok(())
}

#[inline]
pub fn onscreen_keyboard_top_y_norm(state: &EngineState) -> f32 {
    let (_, y1, _, _) = state.keyboard.onscreen.top_edge_coordinates();
    y1
}

/// When the OSK is visible, true if the current pointer is in the keyboard band (pixels `y ≥ top`).
/// Matches embedded [`TextApp::on_mouse_down`] OSK handling so key presses don't retarget [`xos.ui.Text.is_focused`].
#[inline]
pub fn pointer_mouse_in_shown_osk_strip(state: &EngineState) -> bool {
    if !state.keyboard.onscreen.is_shown() {
        return false;
    }
    let shape = state.frame.shape();
    let height = shape[0].max(1) as f32;
    let (_, top_norm, _, _) = state.keyboard.onscreen.top_edge_coordinates();
    let keyboard_band_top_px = top_norm * height;
    state.mouse.y >= keyboard_band_top_px
}
