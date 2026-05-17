//! Native backing for [`xos.ui.whiteboard`]: registry of [`WhiteboardWidget`] surfaces + event routing.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use rustpython_vm::PyObjectRef;
use rustpython_vm::{PyResult, VirtualMachine};

use crate::apps::whiteboard::kernel::WhiteboardWidget;
use crate::engine::EngineState;
use crate::python_api::python_text::{parse_app_xos_event, PyUiEventKind};

static NEXT_WIDGET_ID: AtomicU64 = AtomicU64::new(1);
static REGISTRY: Mutex<Option<HashMap<u64, WhiteboardWidget>>> = Mutex::new(None);

fn registry_mut() -> std::sync::MutexGuard<'static, Option<HashMap<u64, WhiteboardWidget>>> {
    REGISTRY
        .lock()
        .expect("python whiteboard registry mutex poisoned")
}

fn ensure_registry<'a>(
    g: &'a mut std::sync::MutexGuard<'static, Option<HashMap<u64, WhiteboardWidget>>>,
) -> &'a mut HashMap<u64, WhiteboardWidget> {
    if g.is_none() {
        **g = Some(HashMap::new());
    }
    g.as_mut().unwrap()
}

pub fn alloc_widget_id() -> u64 {
    NEXT_WIDGET_ID.fetch_add(1, Ordering::Relaxed)
}

pub fn insert_widget(id: u64, board: WhiteboardWidget) {
    let mut g = registry_mut();
    ensure_registry(&mut g).insert(id, board);
}

pub fn sync_embed_norm_rect(
    id: u64,
    state: &EngineState,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
) -> Result<(), &'static str> {
    let shape = state.frame.shape();
    let fw = shape[1].max(1) as f32;
    let fh = shape[0].max(1) as f32;

    let mut g = registry_mut();
    let Some(map) = g.as_mut() else {
        return Err("whiteboard widget registry unavailable");
    };
    let Some(w) = map.get_mut(&id) else {
        return Err("unknown native whiteboard widget id");
    };
    w.sync_norm_rect(x1, y1, x2, y2)?;
    w.sync_python_viewport_from_norm(fw, fh);
    Ok(())
}

pub fn tick_whiteboard_widget(
    id: u64,
    state: &mut EngineState,
    draw_color: (u8, u8, u8),
    stroke_width: f32,
    editable: bool,
    scrollable_x: bool,
    scrollable_y: bool,
    zoomable: bool,
) {
    let shape = state.frame.shape();
    let fw = shape[1].max(1) as f32;
    let fh = shape[0].max(1) as f32;

    let mut g = registry_mut();
    let Some(map) = g.as_mut() else {
        return;
    };
    let Some(w) = map.get_mut(&id) else {
        return;
    };
    w.sync_python_viewport_from_norm(fw, fh);
    w.set_draw_style(draw_color, stroke_width);
    w.set_interaction_flags(editable, scrollable_x, scrollable_y, zoomable);
    w.tick(state);
}

pub fn render_whiteboard_widget(id: u64, state: &mut EngineState) {
    let shape = state.frame.shape();
    let fw = shape[1] as u32;
    let fh = shape[0] as u32;
    let mouse_x = state.mouse.x;
    let mouse_y = state.mouse.y;
    let is_left = state.mouse.is_left_clicking;
    let is_right = state.mouse.is_right_clicking;

    let g = registry_mut();
    let Some(map) = g.as_ref() else {
        return;
    };
    let Some(w) = map.get(&id) else {
        return;
    };
    let buf = state.frame.buffer_mut();
    w.paint_into_frame(
        buf,
        fw.max(1),
        fh.max(1),
        mouse_x,
        mouse_y,
        is_left,
        is_right,
    );
}

pub fn dispatch_whiteboard_widget(id: u64, kind: PyUiEventKind, state: &mut EngineState) {
    let mut g = registry_mut();
    let Some(map) = g.as_mut() else {
        return;
    };
    let Some(w) = map.get_mut(&id) else {
        return;
    };

    let mx = state.mouse.x;
    let my = state.mouse.y;
    let in_viewport = w.viewport_contains(mx, my);

    match kind {
        PyUiEventKind::Scroll { dx, dy, unit } => {
            if in_viewport {
                w.on_scroll(state, dx, dy, unit);
            }
        }
        _ => {}
    }
}

pub fn dispatch_whiteboard_widget_from_app(
    vm: &VirtualMachine,
    widget_id: u64,
    app: PyObjectRef,
) -> PyResult<()> {
    let Some(kind) = parse_app_xos_event(vm, &app)? else {
        return Ok(());
    };
    crate::python_api::engine::py_engine_tls::with_callback_engine_state_mut(|state| {
        dispatch_whiteboard_widget(widget_id, kind, state);
    });
    Ok(())
}
