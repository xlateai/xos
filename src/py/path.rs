//! `xos.path` — data directory and optional repo root (for bundled dev assets).

use rustpython_vm::{PyObjectRef, PyRef, PyResult, VirtualMachine, builtins::PyModule};

#[cfg(not(target_arch = "wasm32"))]
fn path_data(vm: &VirtualMachine) -> PyResult<String> {
    crate::auth::auth_data_dir()
        .map_err(|e| vm.new_runtime_error(e.to_string()))
        .map(|p| p.to_string_lossy().to_string())
}

/// Repository root (dev / `cargo` builds). `None` on iOS, embedded, or `cargo install` when no
/// checkout is on disk.
#[cfg(not(target_arch = "wasm32"))]
fn path_code(vm: &VirtualMachine) -> PyObjectRef {
    match crate::find_xos_project_root() {
        Ok(p) => vm.new_pyobj(p.to_string_lossy().to_string()),
        Err(_) => vm.ctx.none(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn make_path_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let m = vm.new_module("xos.path", vm.ctx.new_dict(), None);
    let _ = m.set_attr("data", vm.new_function("data", path_data), vm);
    let _ = m.set_attr("code", vm.new_function("code", path_code), vm);
    m
}

#[cfg(target_arch = "wasm32")]
pub fn make_path_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let m = vm.new_module("xos.path", vm.ctx.new_dict(), None);
    let _ = m.set_attr(
        "data",
        vm.new_function("data", |vm: &VirtualMachine| {
            Err(vm.new_runtime_error(
                "xos.path.data: not available on wasm (pass explicit model paths)".to_string(),
            ))
        }),
        vm,
    );
    let _ = m.set_attr("code", vm.new_function("code", |vm: &VirtualMachine| vm.ctx.none()), vm);
    m
}
