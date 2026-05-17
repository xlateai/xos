//! Shared Python `_xos_event` parsing for native text/whiteboard widgets.

use rustpython_vm::builtins::PyDict;
use rustpython_vm::{PyObjectRef, PyResult, VirtualMachine};
use xos_core::engine::keyboard::shortcuts::ShortcutAction;
use xos_core::engine::ScrollWheelUnit;

#[derive(Clone, Copy, Debug)]
pub enum PyUiEventKind {
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

pub fn parse_app_xos_event(vm: &VirtualMachine, app: &PyObjectRef) -> PyResult<Option<PyUiEventKind>> {
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
                return Err(vm.new_value_error(
                    "_xos_event char must be a single unicode character".to_string(),
                ));
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
