//! Hook for building the Python `xos.ui` module (implemented in `xos-app`).

use rustpython_vm::{builtins::PyModule, PyRef, VirtualMachine};
use std::sync::{Mutex, OnceLock};

type MakeUiFn = fn(&VirtualMachine, PyRef<PyModule>) -> PyRef<PyModule>;

static HOOK: OnceLock<Mutex<Option<MakeUiFn>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<MakeUiFn>> {
    HOOK.get_or_init(|| Mutex::new(None))
}

pub fn register_make_ui_module(f: MakeUiFn) {
    *slot().lock().unwrap() = Some(f);
}

pub fn make_ui_module(vm: &VirtualMachine, coordinates: PyRef<PyModule>) -> PyRef<PyModule> {
    let f = slot()
        .lock()
        .unwrap()
        .expect("xos.ui hook not registered (xos-app init_hooks)");
    f(vm, coordinates)
}
