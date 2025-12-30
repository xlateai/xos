use rustpython_vm::{Interpreter, PyObjectRef, AsObject, VirtualMachine, builtins::PyBaseExceptionRef};
use crate::engine::{Application, EngineState};

/// Format a Python exception with traceback info
fn format_python_exception(vm: &VirtualMachine, py_exc: &PyBaseExceptionRef) -> String {
    let mut output = String::new();
    
    // Try to show traceback info if available
    if let Some(traceback) = py_exc.traceback() {
        output.push_str("Traceback (most recent call last):\n");
        
        // Use the debug format which should show file/line info
        let tb_str = format!("{:?}", traceback);
        if !tb_str.is_empty() && tb_str.len() < 500 {
            // Try to extract useful info from the debug string
            for line in tb_str.lines() {
                if line.contains("File") || line.contains("line") {
                    output.push_str("  ");
                    output.push_str(line.trim());
                    output.push('\n');
                }
            }
        }
    }
    
    // Get exception class name
    let class_name = py_exc.class().name().to_string();
    
    // Try to get the exception message
    let msg = vm.call_method(py_exc.as_object(), "__str__", ())
        .ok()
        .and_then(|result| result.str(vm).ok().map(|s| s.to_string()))
        .unwrap_or_default();
    
    // Add exception info
    if !msg.is_empty() {
        output.push_str(&format!("{}: {}", class_name, msg));
    } else {
        output.push_str(&class_name);
    }
    
    output
}

pub const APPLICATION_CLASS_CODE: &str = r#"
class _ArrayResult:
    """Wrapper for list results that provides nice string representation"""
    def __init__(self, data, shape=None):
        self._data = data
        self._shape = shape
    
    def __iter__(self):
        return iter(self._data)
    
    def __len__(self):
        return len(self._data)
    
    def __getitem__(self, idx):
        return self._data[idx]
    
    def __str__(self):
        if not self._data:
            return "xos.Array(empty)"
        min_val = min(self._data)
        max_val = max(self._data)
        mean_val = sum(self._data) / len(self._data)
        shape_str = f"shape={self._shape}" if self._shape else f"len={len(self._data)}"
        return f"xos.Array({shape_str}, min={min_val:.1f}, mean={mean_val:.1f}, max={max_val:.1f})"
    
    def __repr__(self):
        return self.__str__()

class _ArrayWrapper:
    """Wrapper for array dict that supports slice assignment"""
    def __init__(self, data):
        self._data = data
    
    def __getitem__(self, key):
        if isinstance(key, slice) and key == slice(None, None, None):
            # Return the underlying data dict for full slice access
            return self._data
        return self._data[key]
    
    def __setitem__(self, key, value):
        if isinstance(key, slice) and key == slice(None, None, None):
            # Full slice assignment
            # Check if value is a sentinel dict indicating direct fill already happened
            if isinstance(value, dict) and value.get('_direct_fill', False):
                # Data already written directly to buffer by Rust - ZERO COPY! Do nothing.
                return
            # Call Rust function to fill buffer (handles lists and _ArrayResult)
            import xos
            xos.rasterizer._fill_buffer(self._data, value)
        else:
            self._data[key] = value
    
    @property
    def shape(self):
        return self._data.get('shape', ())
    
    @property
    def dtype(self):
        return self._data.get('dtype', 'unknown')
    
    def size(self):
        """Return the total number of elements (product of all dimensions)"""
        shape = self.shape
        if not shape:
            return 0
        size = 1
        for dim in shape:
            size *= dim
        return size
    
    def __str__(self):
        # For regular arrays (not frame arrays), compute statistics
        if '_data' in self._data:
            data_list = self._data['_data']
            if data_list and len(data_list) > 0:
                min_val = min(data_list)
                max_val = max(data_list)
                mean_val = sum(data_list) / len(data_list)
                return f"xos.Array(shape={self.shape}, dtype={self.dtype}, min={min_val:.3f}, max={max_val:.3f}, mean={mean_val:.3f})"
            return f"xos.Array(shape={self.shape}, dtype={self.dtype}, empty)"
        # For frame arrays (no _data field)
        return f"xos.Array(shape={self.shape}, dtype=u8)"
    
    def __repr__(self):
        return self.__str__()

class _FrameWrapper:
    """Wrapper to make frame dict behave like an object with methods"""
    def __init__(self, data):
        self._data = data
        self._array_wrapper = _ArrayWrapper(data.get('array', {}))
    
    @property
    def array(self):
        """Get the array wrapper with slice assignment support"""
        return self._array_wrapper
    
    def get_width(self):
        return self._data['width']
    
    def get_height(self):
        return self._data['height']
    
    def __getitem__(self, key):
        return self._data[key]
    
    def __setitem__(self, key, value):
        self._data[key] = value

class Application:
    """Base class for xos applications. Extend this class and implement setup() and tick()."""
    
    def __init__(self):
        self.frame = None  # Will be set by the engine
        self.mouse = None  # Will be set by the engine
    
    def get_width(self):
        """Get the current frame width"""
        return self.frame.get_width()
    
    def get_height(self):
        """Get the current frame height"""
        return self.frame.get_height()
    
    def setup(self):
        """Called once when the application starts. Override this method."""
        raise NotImplementedError("Subclasses must implement setup()")
    
    def tick(self):
        """Called every frame. Override this method."""
        raise NotImplementedError("Subclasses must implement tick()")
    
    def on_mouse_down(self, x, y):
        """Called when mouse is clicked. Override this method (optional)."""
        pass
    
    def on_mouse_up(self, x, y):
        """Called when mouse is released. Override this method (optional)."""
        pass
    
    def on_mouse_move(self, x, y):
        """Called when mouse moves. Override this method (optional)."""
        pass
    
    def on_screen_size_change(self, width, height):
        """Called when screen size changes. Override this method (optional)."""
        pass
    
    def run(self):
        """Run the application with the xos engine."""
        # Store self in builtins so Rust can find it from any scope
        import builtins
        builtins.__xos_app_instance__ = self
        print("[xos] Application instance registered, engine will launch...")
"#;

/// PyApp wraps a Python Application instance and implements the Rust Application trait
pub struct PyApp {
    interpreter: Interpreter,
    app_instance: Option<PyObjectRef>,
}

impl PyApp {
    pub fn new(interpreter: Interpreter, app_instance: PyObjectRef) -> Self {
        Self {
            interpreter,
            app_instance: Some(app_instance),
        }
    }
}

impl Application for PyApp {
    fn setup(&mut self, state: &mut EngineState) -> Result<(), String> {
        if let Some(ref app_instance) = self.app_instance {
            self.interpreter.enter(|vm| {
                // Create Python frame object from engine state
                let frame_dict = crate::python::engine::py_bindings::create_py_frame_state(vm, &mut state.frame)
                    .map_err(|e| format!("Failed to create frame object: {:?}", e))?;
                
                // Wrap it in _FrameWrapper
                if let Ok(wrapper_class) = vm.builtins.get_attr("_FrameWrapper", vm) {
                    // Use the newer call API instead of deprecated invoke
                    if let Ok(frame_obj) = wrapper_class.call((frame_dict.clone(),), vm) {
                        app_instance.set_attr("frame", frame_obj, vm)
                            .map_err(|e| format!("Failed to set frame attribute: {:?}", e))?;
                    } else {
                        // Fallback: just use the dict directly
                        app_instance.set_attr("frame", frame_dict, vm)
                            .map_err(|e| format!("Failed to set frame attribute: {:?}", e))?;
                    }
                } else {
                    // Fallback: just use the dict directly
                    app_instance.set_attr("frame", frame_dict, vm)
                        .map_err(|e| format!("Failed to set frame attribute: {:?}", e))?;
                }
                
                // Create mouse object
                let mouse_dict = vm.ctx.new_dict();
                mouse_dict.set_item("x", vm.ctx.new_float(state.mouse.x as f64).into(), vm)
                    .map_err(|e| format!("Mouse x error: {:?}", e))?;
                mouse_dict.set_item("y", vm.ctx.new_float(state.mouse.y as f64).into(), vm)
                    .map_err(|e| format!("Mouse y error: {:?}", e))?;
                mouse_dict.set_item("is_left_clicking", vm.ctx.new_bool(state.mouse.is_left_clicking).into(), vm)
                    .map_err(|e| format!("Mouse clicking error: {:?}", e))?;
                
                app_instance.set_attr("mouse", mouse_dict, vm)
                    .map_err(|e| format!("Failed to set mouse attribute: {:?}", e))?;
                
                // Call setup
                match vm.call_method(app_instance, "setup", ()) {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        let error_msg = format_python_exception(vm, &e);
                        Err(format!("Python setup error:\n{}", error_msg))
                    }
                }
            })
        } else {
            Err("No Python app instance".to_string())
        }
    }

    fn tick(&mut self, state: &mut EngineState) {
        if let Some(ref app_instance) = self.app_instance {
            // Set the frame buffer context for the rasterizer
            let shape = state.frame.shape();
            let width = shape[1];
            let height = shape[0];
            let buffer = state.frame.buffer_mut();
            crate::python::rasterizer::set_frame_buffer_context(buffer, width, height);
            
            self.interpreter.enter(|vm| {
                // Update frame data before calling tick
                if let Ok(Some(frame_obj)) = vm.get_attribute_opt(app_instance.clone(), "frame") {
                    let _ = crate::python::engine::py_bindings::update_py_frame_state(vm, frame_obj.clone(), &mut state.frame);
                    
                    // Update mouse data
                    let mouse_dict = vm.ctx.new_dict();
                    let _ = mouse_dict.set_item("x", vm.ctx.new_float(state.mouse.x as f64).into(), vm);
                    let _ = mouse_dict.set_item("y", vm.ctx.new_float(state.mouse.y as f64).into(), vm);
                    let _ = mouse_dict.set_item("is_left_clicking", vm.ctx.new_bool(state.mouse.is_left_clicking).into(), vm);
                    let _ = app_instance.set_attr("mouse", mouse_dict, vm);
                    
                    // Call tick
                    if let Err(e) = vm.call_method(app_instance, "tick", ()) {
                        let error_msg = format_python_exception(vm, &e);
                        eprintln!("Python tick error:\n{}", error_msg);
                    }
                }
            });
            
            // Clear the frame buffer context after tick
            crate::python::rasterizer::clear_frame_buffer_context();
        }
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        if let Some(ref app_instance) = self.app_instance {
            self.interpreter.enter(|vm| {
                let x = state.mouse.x;
                let y = state.mouse.y;
                let _ = vm.call_method(app_instance, "on_mouse_down", (x, y));
            });
        }
    }

    fn on_mouse_up(&mut self, state: &mut EngineState) {
        if let Some(ref app_instance) = self.app_instance {
            self.interpreter.enter(|vm| {
                let x = state.mouse.x;
                let y = state.mouse.y;
                let _ = vm.call_method(app_instance, "on_mouse_up", (x, y));
            });
        }
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        if let Some(ref app_instance) = self.app_instance {
            self.interpreter.enter(|vm| {
                let x = state.mouse.x;
                let y = state.mouse.y;
                let _ = vm.call_method(app_instance, "on_mouse_move", (x, y));
            });
        }
    }

    fn on_screen_size_change(&mut self, state: &mut EngineState, width: u32, height: u32) {
        if let Some(ref app_instance) = self.app_instance {
            // Set the frame buffer context so Python can write to it
            let shape = state.frame.shape();
            let frame_width = shape[1];
            let frame_height = shape[0];
            let buffer = state.frame.buffer_mut();
            crate::python::rasterizer::set_frame_buffer_context(buffer, frame_width, frame_height);
            
            self.interpreter.enter(|vm| {
                // Update frame data before calling the handler
                if let Ok(Some(frame_obj)) = vm.get_attribute_opt(app_instance.clone(), "frame") {
                    let _ = crate::python::engine::py_bindings::update_py_frame_state(vm, frame_obj, &mut state.frame);
                }
                // Call the Python handler
                let _ = vm.call_method(app_instance, "on_screen_size_change", (width, height));
            });
            
            // Clear the frame buffer context after handler completes
            crate::python::rasterizer::clear_frame_buffer_context();
        }
    }
}
