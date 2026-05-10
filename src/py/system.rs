use rustpython_vm::{builtins::PyModule, function::FuncArgs, PyRef, PyResult, VirtualMachine};
use rustpython_vm::AsObject;

use crate::apps::remote::monitors::{self, MonitorDescriptor};
use crate::python_api::json_codec::py_frame_from_rgba_bytes;

/// System type enum - matches target OS
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemType {
    IOS,
    MacOS,
    Windows,
    Linux,
    Wasm,
}

impl SystemType {
    pub fn name(&self) -> &'static str {
        match self {
            SystemType::IOS => "IOS",
            SystemType::MacOS => "MacOS",
            SystemType::Windows => "Windows",
            SystemType::Linux => "Linux",
            SystemType::Wasm => "Wasm",
        }
    }

    /// Get the current system type
    pub fn current() -> Self {
        #[cfg(target_os = "ios")]
        return SystemType::IOS;

        #[cfg(all(target_os = "macos", not(target_os = "ios")))]
        return SystemType::MacOS;

        #[cfg(target_os = "windows")]
        return SystemType::Windows;

        #[cfg(target_os = "linux")]
        return SystemType::Linux;

        #[cfg(target_arch = "wasm32")]
        return SystemType::Wasm;
    }
}

/// Python system type class code
pub const SYSTEM_TYPE_CLASS_CODE: &str = r#"
class SystemType:
    """System type descriptor for xos"""
    def __init__(self, name):
        self.name = name
    
    def __str__(self):
        return f"xos.system.types.{self.name}"
    
    def __repr__(self):
        return self.__str__()
    
    def __eq__(self, other):
        if isinstance(other, SystemType):
            return self.name == other.name
        return False
    
    def __ne__(self, other):
        return not self.__eq__(other)
"#;

const SYSTEM_MONITORS_BOOTSTRAP: &str = include_str!("monitors_bootstrap.py");

#[cfg(all(
    not(target_arch = "wasm32"),
    any(target_os = "macos", target_os = "windows")
))]
fn desktop_monitor_meta(idx: usize) -> Option<MonitorDescriptor> {
    monitors::system_monitors().get(idx).cloned()
}

#[cfg(not(all(
    not(target_arch = "wasm32"),
    any(target_os = "macos", target_os = "windows")
)))]
fn desktop_monitor_meta(_idx: usize) -> Option<MonitorDescriptor> {
    None
}

fn system_monitor_first_arg_usize(args: &FuncArgs, vm: &VirtualMachine) -> PyResult<usize> {
    let obj = args
        .args
        .get(0)
        .ok_or_else(|| vm.new_type_error("monitor op requires integer index argument".into()))?;
    let i: isize = obj.clone().try_into_value(vm)?;
    if i < 0 {
        return Err(vm.new_index_error("monitor index must be >= 0".into()));
    }
    usize::try_from(i).map_err(|_| vm.new_index_error("monitor index overflow".into()))
}

fn monitors_len(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    #[cfg(all(
        not(target_arch = "wasm32"),
        any(target_os = "macos", target_os = "windows")
    ))]
    let n = monitors::system_monitors().len();
    #[cfg(not(all(
        not(target_arch = "wasm32"),
        any(target_os = "macos", target_os = "windows")
    )))]
    let n = 0usize;
    Ok(vm.ctx.new_int(n as isize).into())
}

fn monitor_meta_dict(vm: &VirtualMachine, m: &MonitorDescriptor) -> PyResult {
    let d = vm.ctx.new_dict();
    d.set_item(
        "native_width",
        vm.ctx.new_int(m.native_width as isize).into(),
        vm,
    )?;
    d.set_item(
        "native_height",
        vm.ctx.new_int(m.native_height as isize).into(),
        vm,
    )?;
    d.set_item(
        "stream_width",
        vm.ctx.new_int(m.stream_width as isize).into(),
        vm,
    )?;
    d.set_item(
        "stream_height",
        vm.ctx.new_int(m.stream_height as isize).into(),
        vm,
    )?;
    d.set_item("origin_x", vm.ctx.new_int(m.origin_x as isize).into(), vm)?;
    d.set_item("origin_y", vm.ctx.new_int(m.origin_y as isize).into(), vm)?;
    d.set_item("refresh_rate_hz", vm.ctx.new_float(m.refresh_rate_hz).into(), vm)?;
    d.set_item("is_primary", vm.ctx.new_bool(m.is_primary).into(), vm)?;
    d.set_item("name", vm.ctx.new_str(m.name.as_str()).into(), vm)?;
    d.set_item(
        "native_id",
        vm.ctx.new_str(m.native_id.as_str()).into(),
        vm,
    )?;
    Ok(d.into())
}

fn system_monitor_meta(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let idx = system_monitor_first_arg_usize(&args, vm)?;
    let Some(m) = desktop_monitor_meta(idx) else {
        return Err(vm.new_index_error("invalid monitor index".into()));
    };
    monitor_meta_dict(vm, &m)
}

fn system_monitor_get_frame(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    #[cfg(not(all(
        not(target_arch = "wasm32"),
        any(target_os = "macos", target_os = "windows")
    )))]
    {
        return Err(vm.new_runtime_error(
            "xos.system.Monitor.get_frame is only available on native macOS and Windows builds"
                .into(),
        ));
    }
    #[cfg(all(
        not(target_arch = "wasm32"),
        any(target_os = "macos", target_os = "windows")
    ))]
    {
        let idx = system_monitor_first_arg_usize(&args, vm)?;
        let Some((rgba, w, h)) = crate::apps::remote::monitor_stream::snapshot(idx)
            .or_else(|| monitors::system_monitor_capture_scaled_rgba(idx))
        else {
            return Err(vm.new_runtime_error(
                "monitor get_frame failed (bad index, permissions, or capture driver)".into(),
            ));
        };
        py_frame_from_rgba_bytes(vm, w as usize, h as usize, rgba)
    }
}

/// xos.system.get_system_type() - Get current system type
fn get_system_type(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let system_type = SystemType::current();

    // Directly create the system type string and return it
    // The actual comparison will happen in Python code
    Ok(vm.ctx.new_str(system_type.name()).into())
}

/// Create the system module with SystemType constants
pub fn make_system_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.system", vm.ctx.new_dict(), None);

    // Execute the SystemType class definition
    let scope = vm.new_scope_with_builtins();
    if let Err(e) = vm.run_code_string(
        scope.clone(),
        SYSTEM_TYPE_CLASS_CODE,
        "<system>".to_string(),
    ) {
        eprintln!("Failed to create SystemType class: {:?}", e);
        return module;
    }

    // Get the SystemType class
    let system_type_class = match scope.globals.get_item("SystemType", vm) {
        Ok(cls) => cls,
        Err(_) => return module,
    };

    // Create a types namespace using Python code
    let types_code = r#"
class _TypesNamespace:
    pass

_types = _TypesNamespace()
"#;

    if let Err(e) = vm.run_code_string(scope.clone(), types_code, "<system_types>".to_string()) {
        eprintln!("Failed to create types namespace: {:?}", e);
        return module;
    }

    let types_ns = match scope.globals.get_item("_types", vm) {
        Ok(ns) => ns,
        Err(_) => return module,
    };

    // Helper to create system type instances
    let create_system_type = |name: &str| -> Option<rustpython_vm::PyObjectRef> {
        let name_str: rustpython_vm::PyObjectRef = vm.ctx.new_str(name).into();
        system_type_class.call((name_str,), vm).ok()
    };

    // Create all system type constants and add to types namespace
    if let Some(st) = create_system_type("IOS") {
        let _ = types_ns.set_attr("IOS", st, vm);
    }
    if let Some(st) = create_system_type("MacOS") {
        let _ = types_ns.set_attr("MacOS", st, vm);
    }
    if let Some(st) = create_system_type("Windows") {
        let _ = types_ns.set_attr("Windows", st, vm);
    }
    if let Some(st) = create_system_type("Linux") {
        let _ = types_ns.set_attr("Linux", st, vm);
    }
    if let Some(st) = create_system_type("Wasm") {
        let _ = types_ns.set_attr("Wasm", st, vm);
    }

    // Add types namespace as an attribute
    let _ = module.set_attr("types", types_ns, vm);

    // Add get_system_type function
    let _ = module.set_attr(
        "get_system_type",
        vm.new_function("get_system_type", get_system_type),
        vm,
    );

    let mon_scope = vm.new_scope_with_builtins();
    let _ = mon_scope.globals.set_item(
        "_system_monitor_len",
        vm.new_function("_system_monitor_len", monitors_len).into(),
        vm,
    );
    let _ = mon_scope.globals.set_item(
        "_system_monitor_meta",
        vm.new_function("_system_monitor_meta", system_monitor_meta).into(),
        vm,
    );
    let _ = mon_scope.globals.set_item(
        "_system_monitor_get_frame",
        vm.new_function("_system_monitor_get_frame", system_monitor_get_frame).into(),
        vm,
    );
    match vm.run_code_string(
        mon_scope.clone(),
        SYSTEM_MONITORS_BOOTSTRAP,
        "<xos.system/monitors_bootstrap.py>".to_string(),
    ) {
        Ok(_) => {
            if let Ok(v) = mon_scope.globals.get_item("Monitor", vm) {
                let _ = module.set_attr("Monitor", v, vm);
            }
            if let Ok(v) = mon_scope.globals.get_item("monitors", vm) {
                let _ = module.set_attr("monitors", v, vm);
            }
        }
        Err(e) => {
            eprintln!("xos.system monitors bootstrap failed: {e:?}");
        }
    }

    module
}
