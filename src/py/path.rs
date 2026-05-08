//! `xos.path` — data directory and optional repo root (for bundled dev assets).

#[cfg(not(target_arch = "wasm32"))]
use rustpython_vm::PyObjectRef;
use rustpython_vm::{builtins::PyModule, function::FuncArgs, PyRef, PyResult, VirtualMachine};

fn path_fs_exists(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let s: String = args.bind(vm)?;
    Ok(vm.ctx.new_bool(crate::fs::exists(&s)).into())
}

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
    if crate::fs::exists(&path_s) {
        return if exists_ok && crate::fs::is_dir(&path_s) {
            Ok(vm.ctx.none())
        } else {
            Err(vm.new_os_error(if exists_ok {
                format!(
                    "cannot makedirs {:?}: exists and is not a directory",
                    path_s
                )
            } else {
                format!("path already exists: {}", path_s)
            }))
        };
    }
    crate::fs::create_dir_all(&path_s).map_err(|e| vm.new_os_error(e))?;
    Ok(vm.ctx.none())
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

    let ef = m
        .get_attr("__native_path_exists", vm)
        .map_err(|_| "path.exists bind")?;
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
    vm.run_code_string(
        scope.clone(),
        full.as_str(),
        "<xos.path/dotxos>".to_string(),
    )
    .map_err(|_| "xos.path/dotxos exec")?;
    if let Ok(d) = scope.globals.get_item("dotxos", vm) {
        m.set_attr("dotxos", d, vm).map_err(|_| "set dotxos")?;
    }
    Ok(())
}

fn path_data(vm: &VirtualMachine) -> PyResult<String> {
    crate::fs::data_dir_string().map_err(|e| vm.new_runtime_error(e))
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

#[cfg(target_arch = "wasm32")]
fn path_code(vm: &VirtualMachine) -> PyResult {
    Ok(vm.ctx.none())
}

pub fn make_path_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let m = vm.new_module("xos.path", vm.ctx.new_dict(), None);
    let _ = m.set_attr("data", vm.new_function("data", path_data), vm);
    let _ = m.set_attr("code", vm.new_function("code", path_code), vm);

    fn data_dir_py(vm: &VirtualMachine) -> PyResult {
        path_data(vm).map(|s| vm.ctx.new_str(s.as_str()).into())
    }
    let _ = m.set_attr(
        "_data_dir_str",
        vm.new_function("_data_dir_str", data_dir_py),
        vm,
    );

    let _ = inject_dotxos(vm, &m, "dotxos = _DataPath(str(_data_dir_str()))").expect("dotxos");
    m
}
