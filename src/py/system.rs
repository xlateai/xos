use rustpython_vm::{PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs};

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
    if let Err(e) = vm.run_code_string(scope.clone(), SYSTEM_TYPE_CLASS_CODE, "<system>".to_string()) {
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
    let _ = module.set_attr("get_system_type", vm.new_function("get_system_type", get_system_type), vm);
    
    module
}

