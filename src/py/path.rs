//! `xos.path` — data directory and optional repo root (for bundled dev assets).

#[cfg(not(target_arch = "wasm32"))]
use rustpython_vm::PyObjectRef;
use rustpython_vm::{PyRef, PyResult, VirtualMachine, builtins::PyModule, function::FuncArgs};

#[cfg(not(target_arch = "wasm32"))]
fn path_fs_exists(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let s: String = args.bind(vm)?;
    Ok(vm.ctx.new_bool(std::path::Path::new(&s).exists()).into())
}

#[cfg(target_arch = "wasm32")]
fn path_fs_exists(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    Ok(vm.ctx.new_bool(false).into())
}

#[cfg(not(target_arch = "wasm32"))]
fn path_fs_makedirs(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let av = args.args.as_slice();
    if av.is_empty() || av.len() > 2 {
        return Err(vm.new_type_error(
            "__native_path_makedirs(path[, exists_ok]) expects 1 or 2 positional args".into(),
        ));
    }
    let path_s: String = av[0].clone().try_into_value(vm)?;
    let exists_ok = av
        .get(1)
        .map(|o| o.clone().try_into_value::<bool>(vm))
        .transpose()?
        .unwrap_or(false);
    let p = std::path::Path::new(&path_s);
    if p.exists() {
        return if exists_ok && p.is_dir() {
            Ok(vm.ctx.none())
        } else {
            Err(vm.new_os_error(if exists_ok {
                format!("cannot makedirs {:?}: exists and is not a directory", path_s)
            } else {
                format!("path already exists: {}", path_s)
            }))
        };
    }
    std::fs::create_dir_all(p).map_err(|e| vm.new_os_error(format!("makedirs {:?}: {}", path_s, e)))?;
    Ok(vm.ctx.none())
}

#[cfg(target_arch = "wasm32")]
fn path_fs_makedirs(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    Err(vm.new_runtime_error(
        "xos.path: makedirs is not available on wasm builds".into(),
    ))
}

const DATAPATH_BODY: &str = r#"
# Avoid ``__native_*`` inside ``_DataPath`` methods — Python would mangle to ``_DataPath__native_*``.
_native_path_exists = __native_path_exists
_native_path_makedirs = __native_path_makedirs
class _DataPath:
    __slots__ = ("_s",)
    def __init__(self, s):
        object.__setattr__(self, "_s", str(s).replace("\\", "/").rstrip("/"))
    def __truediv__(self, other):
        o = str(other).strip("/").replace("\\", "/")
        if not o:
            return self
        return _DataPath(self._s + "/" + o)
    def __str__(self):
        return self._s
    def __repr__(self):
        return f"DataPath({self._s!r})"
    def __fspath__(self):
        return self._s
    def exists(self):
        return bool(_native_path_exists(str(self)))
    def makedirs(self, exists_ok=False):
        _native_path_makedirs(str(self), bool(exists_ok))
"#;

fn inject_dotxos(
    vm: &VirtualMachine,
    m: &PyRef<PyModule>,
    init_call: &str,
) -> Result<(), &'static str> {
    let _ = m.set_attr(
        "__native_path_exists",
        vm.new_function("__native_path_exists", path_fs_exists),
        vm,
    );
    let _ = m.set_attr(
        "__native_path_makedirs",
        vm.new_function("__native_path_makedirs", path_fs_makedirs),
        vm,
    );

    let scope = vm.new_scope_with_builtins();
    if let Ok(f) = m.get_attr("_data_dir_str", vm) {
        let _ = scope.globals.set_item("_data_dir_str", f, vm);
    }

    let ef = m.get_attr("__native_path_exists", vm).map_err(|_| "path.exists bind")?;
    scope
        .globals
        .set_item("__native_path_exists", ef, vm)
        .map_err(|_| "path globals exists")?;
    let mf = m
        .get_attr("__native_path_makedirs", vm)
        .map_err(|_| "path.makedirs bind")?;
    scope
        .globals
        .set_item("__native_path_makedirs", mf, vm)
        .map_err(|_| "path globals makedirs")?;

    let full = format!("{}\n{}", DATAPATH_BODY, init_call);
    vm.run_code_string(scope.clone(), full.as_str(), "<xos.path/dotxos>".to_string())
        .map_err(|_| "xos.path/dotxos exec")?;
    if let Ok(d) = scope.globals.get_item("dotxos", vm) {
        m.set_attr("dotxos", d, vm).map_err(|_| "set dotxos")?;
    }
    Ok(())
}

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

    fn data_dir_py(vm: &VirtualMachine) -> PyResult {
        path_data(vm).map(|s| vm.ctx.new_str(s.as_str()).into())
    }
    let _ = m.set_attr("_data_dir_str", vm.new_function("_data_dir_str", data_dir_py), vm);

    let _ = inject_dotxos(vm, &m, "dotxos = _DataPath(str(_data_dir_str()))").expect("dotxos");
    m
}

#[cfg(target_arch = "wasm32")]
pub fn make_path_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let m = vm.new_module("xos.path", vm.ctx.new_dict(), None);
    let _ = m.set_attr(
        "data",
        vm.new_function("data", |vm: &VirtualMachine| -> PyResult<String> {
            Err(vm.new_runtime_error(
                "xos.path.data: not available on wasm (pass explicit model paths)".to_string(),
            ))
        }),
        vm,
    );
    let _ = m.set_attr("code", vm.new_function("code", |vm: &VirtualMachine| vm.ctx.none()), vm);
    let _ = m.set_attr(
        "_data_dir_str",
        vm.new_function("_data_dir_str", |vm: &VirtualMachine| -> PyResult {
            Ok(vm.ctx.new_str(".").into())
        }),
        vm,
    );
    let _ = inject_dotxos(vm, &m, r#"dotxos = _DataPath(".")"#).expect("dotxos wasm");
    m
}
