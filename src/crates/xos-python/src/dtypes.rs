use rustpython_vm::{builtins::PyModule, PyRef, PyResult, VirtualMachine};

/// Data type enum matching NumPy/PyTorch dtypes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DType {
    // Floating point types
    Float16,
    Float32,
    Float64,

    // Signed integer types
    Int8,
    Int16,
    Int32,
    Int64,

    // Unsigned integer types
    UInt8,
    UInt16,
    UInt32,
    UInt64,

    // Boolean
    Bool,
}

impl DType {
    /// Get the string name of the dtype
    pub fn name(&self) -> &'static str {
        match self {
            DType::Float16 => "float16",
            DType::Float32 => "float32",
            DType::Float64 => "float64",
            DType::Int8 => "int8",
            DType::Int16 => "int16",
            DType::Int32 => "int32",
            DType::Int64 => "int64",
            DType::UInt8 => "uint8",
            DType::UInt16 => "uint16",
            DType::UInt32 => "uint32",
            DType::UInt64 => "uint64",
            DType::Bool => "bool",
        }
    }

    /// Get the size in bytes
    pub fn size(&self) -> usize {
        match self {
            DType::Bool | DType::Int8 | DType::UInt8 => 1,
            DType::Float16 | DType::Int16 | DType::UInt16 => 2,
            DType::Float32 | DType::Int32 | DType::UInt32 => 4,
            DType::Float64 | DType::Int64 | DType::UInt64 => 8,
        }
    }

    /// Check if this is a floating point type
    pub fn is_float(&self) -> bool {
        matches!(self, DType::Float16 | DType::Float32 | DType::Float64)
    }

    /// Check if this is an integer type
    pub fn is_int(&self) -> bool {
        matches!(
            self,
            DType::Int8
                | DType::Int16
                | DType::Int32
                | DType::Int64
                | DType::UInt8
                | DType::UInt16
                | DType::UInt32
                | DType::UInt64
        )
    }

    /// Parse from Python object (string name or dtype object)
    pub fn from_py_object(obj: &rustpython_vm::PyObjectRef, vm: &VirtualMachine) -> PyResult<Self> {
        // Try to get the 'name' attribute (if it's a dtype object)
        if let Ok(name_attr) = obj.get_attr("name", vm) {
            if let Ok(s) = name_attr.str(vm) {
                if let Some(dtype) = Self::from_str(&s.to_string()) {
                    return Ok(dtype);
                }
            }
        }

        // Try to convert directly to string
        if let Ok(s) = obj.str(vm) {
            if let Some(dtype) = Self::from_str(&s.to_string()) {
                return Ok(dtype);
            }
        }

        Err(vm.new_type_error("dtype must be a string or dtype object".to_string()))
    }

    /// Parse from string name - returns Option instead of PyResult
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "float16" => Some(DType::Float16),
            "float32" => Some(DType::Float32),
            "float64" | "float" => Some(DType::Float64),
            "int8" => Some(DType::Int8),
            "int16" => Some(DType::Int16),
            "int32" | "int" => Some(DType::Int32),
            "int64" => Some(DType::Int64),
            "uint8" => Some(DType::UInt8),
            "uint16" => Some(DType::UInt16),
            "uint32" => Some(DType::UInt32),
            "uint64" => Some(DType::UInt64),
            "bool" => Some(DType::Bool),
            _ => None,
        }
    }

    /// Convert f32 value to this dtype (as f32 for storage)
    pub fn cast_from_f32(&self, value: f32) -> f32 {
        match self {
            DType::Float16 | DType::Float32 | DType::Float64 => value,
            DType::Int8 => (value as i8) as f32,
            DType::Int16 => (value as i16) as f32,
            DType::Int32 => (value as i32) as f32,
            DType::Int64 => (value as i64) as f32,
            DType::UInt8 => (value as u8) as f32,
            DType::UInt16 => (value as u16) as f32,
            DType::UInt32 => (value as u32) as f32,
            DType::UInt64 => (value as u64) as f32,
            DType::Bool => {
                if value != 0.0 {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

impl Default for DType {
    fn default() -> Self {
        DType::Float32
    }
}

/// Python dtype class code
pub const DTYPE_CLASS_CODE: &str = r#"
class DType:
    """Data type descriptor for xos arrays"""
    def __init__(self, name):
        self.name = name
    
    def __str__(self):
        return f"xos.{self.name}"
    
    def __repr__(self):
        return self.__str__()
"#;

/// Create the dtypes module with all dtype constants
pub fn make_dtypes_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.dtypes", vm.ctx.new_dict(), None);

    // Execute the DType class definition
    let scope = vm.new_scope_with_builtins();
    if let Err(e) = vm.run_code_string(scope.clone(), DTYPE_CLASS_CODE, "<dtypes>".to_string()) {
        eprintln!("Failed to create DType class: {:?}", e);
        return module;
    }

    // Get the DType class
    let dtype_class = match scope.globals.get_item("DType", vm) {
        Ok(cls) => cls,
        Err(_) => return module,
    };

    // Helper to create dtype instances
    let create_dtype = |name: &str| -> Option<rustpython_vm::PyObjectRef> {
        let name_str: rustpython_vm::PyObjectRef = vm.ctx.new_str(name).into();
        dtype_class.call((name_str,), vm).ok()
    };

    // Create all dtype constants
    if let Some(dt) = create_dtype("float16") {
        let _ = module.set_attr("float16", dt, vm);
    }
    if let Some(dt) = create_dtype("float32") {
        let _ = module.set_attr("float32", dt.clone(), vm);
        let _ = module.set_attr("float", dt, vm); // Alias
    }
    if let Some(dt) = create_dtype("float64") {
        let _ = module.set_attr("float64", dt, vm);
    }

    if let Some(dt) = create_dtype("int8") {
        let _ = module.set_attr("int8", dt, vm);
    }
    if let Some(dt) = create_dtype("int16") {
        let _ = module.set_attr("int16", dt, vm);
    }
    if let Some(dt) = create_dtype("int32") {
        let _ = module.set_attr("int32", dt.clone(), vm);
        let _ = module.set_attr("int", dt, vm); // Alias
    }
    if let Some(dt) = create_dtype("int64") {
        let _ = module.set_attr("int64", dt, vm);
    }

    if let Some(dt) = create_dtype("uint8") {
        let _ = module.set_attr("uint8", dt, vm);
    }
    if let Some(dt) = create_dtype("uint16") {
        let _ = module.set_attr("uint16", dt, vm);
    }
    if let Some(dt) = create_dtype("uint32") {
        let _ = module.set_attr("uint32", dt, vm);
    }
    if let Some(dt) = create_dtype("uint64") {
        let _ = module.set_attr("uint64", dt, vm);
    }

    if let Some(dt) = create_dtype("bool") {
        let _ = module.set_attr("bool", dt, vm);
    }

    module
}
