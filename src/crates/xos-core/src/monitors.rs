//! Desktop monitor enumeration hooks (implemented by `xos-app` remote on macOS/Windows).

use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug)]
pub struct MonitorDescriptor {
    pub native_width: u32,
    pub native_height: u32,
    pub origin_x: i32,
    pub origin_y: i32,
    pub refresh_rate_hz: f64,
    pub is_primary: bool,
    pub name: String,
    pub native_id: String,
    pub stream_width: u32,
    pub stream_height: u32,
}

type ListFn = fn() -> Vec<MonitorDescriptor>;
type CaptureFn = fn(usize) -> Option<(Vec<u8>, u32, u32)>;
type SnapshotFn = fn(usize) -> Option<(Vec<u8>, u32, u32)>;

struct Hooks {
    list: Option<ListFn>,
    capture: Option<CaptureFn>,
    snapshot: Option<SnapshotFn>,
}

static HOOKS: OnceLock<Mutex<Hooks>> = OnceLock::new();

fn hooks() -> &'static Mutex<Hooks> {
    HOOKS.get_or_init(|| {
        Mutex::new(Hooks {
            list: None,
            capture: None,
            snapshot: None,
        })
    })
}

pub fn register_monitor_hooks(list: ListFn, capture: CaptureFn, snapshot: SnapshotFn) {
    let mut g = hooks().lock().unwrap();
    g.list = Some(list);
    g.capture = Some(capture);
    g.snapshot = Some(snapshot);
}

pub fn system_monitors() -> Vec<MonitorDescriptor> {
    hooks()
        .lock()
        .unwrap()
        .list
        .map(|f| f())
        .unwrap_or_default()
}

pub fn system_monitor_capture_scaled_rgba(idx: usize) -> Option<(Vec<u8>, u32, u32)> {
    hooks().lock().unwrap().capture.and_then(|f| f(idx))
}

pub fn monitor_snapshot(idx: usize) -> Option<(Vec<u8>, u32, u32)> {
    hooks().lock().unwrap().snapshot.and_then(|f| f(idx))
}
