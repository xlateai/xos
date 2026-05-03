//! Native backing for [`xos.ui.Text`]: registry of [`TextApp`] editors + event routing.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use rustpython_vm::builtins::PyDict;
use rustpython_vm::{PyObjectRef, PyResult, VirtualMachine};

use crate::apps::text::TextApp;
use crate::engine::keyboard::shortcuts::ShortcutAction;
use crate::engine::{Application, EngineState};

/// Monotonic id assignment for [`Text`] handles (Python-visible as `_native_id`).
static NEXT_WIDGET_ID: AtomicU64 = AtomicU64::new(1);

static REGISTRY: Mutex<Option<HashMap<u64, TextApp>>> = Mutex::new(None);

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

/// Snapshot native [`TextApp`] layout, caret, selection, and trackpad laser for [`xos.ui._text_render`].
#[derive(Clone, Debug)]
pub struct EditorRenderPeek {
    pub text: String,
    pub cursor_position: usize,
    pub show_cursor: bool,
    pub font_size_px: f32,
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
        font_size_px: t.text_rasterizer.font_size,
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
    Scroll { dx: f32, dy: f32 },
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
            PyUiEventKind::Scroll { dx, dy }
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
    let mut g = registry_mut();
    let Some(map) = g.as_mut() else {
        return;
    };
    let Some(t) = map.get_mut(&id) else {
        return;
    };
    match kind {
        PyUiEventKind::MouseDown => t.on_mouse_down(state),
        PyUiEventKind::MouseUp => t.on_mouse_up(state),
        PyUiEventKind::MouseMove => t.on_mouse_move(state),
        PyUiEventKind::Scroll { dx, dy } => t.on_scroll(state, dx, dy),
        PyUiEventKind::Key(ch) => t.on_key_char(state, ch),
        PyUiEventKind::Shortcut(sa) => t.apply_keyboard_shortcut(sa, state),
    }
}

pub fn tick_text_widget(id: u64, state: &mut EngineState) {
    let mut g = registry_mut();
    let Some(map) = g.as_mut() else {
        return;
    };
    let Some(t) = map.get_mut(&id) else {
        return;
    };
    t.tick(state);
}
