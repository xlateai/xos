#[cfg(feature = "python")]
use rustpython_vm::{Interpreter, PyObjectRef, AsObject};
use crate::engine::{Application, EngineState};

pub const APPLICATION_CLASS_CODE: &str = r#"
class _FrameWrapper:
    """Wrapper to make frame dict behave like an object with methods"""
    def __init__(self, data):
        self._data = data
    
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
    
    def run(self):
        """Run the application with the xos engine."""
        # Store self in builtins so Rust can find it from any scope
        import builtins
        builtins.__xos_app_instance__ = self
        print("[xos] Application instance registered, engine will launch...")
"#;

/// PyApp wraps a Python Application instance and implements the Rust Application trait
#[cfg(feature = "python")]
pub struct PyApp {
    interpreter: Interpreter,
    app_instance: Option<PyObjectRef>,
}

#[cfg(feature = "python")]
impl PyApp {
    pub fn new(interpreter: Interpreter, app_instance: PyObjectRef) -> Self {
        Self {
            interpreter,
            app_instance: Some(app_instance),
        }
    }
}

#[cfg(feature = "python")]
impl Application for PyApp {
    fn setup(&mut self, state: &mut EngineState) -> Result<(), String> {
        if let Some(ref app_instance) = self.app_instance {
            self.interpreter.enter(|vm| {
                // Create Python frame object from engine state
                let frame_dict = crate::python::engine::py_bindings::create_py_frame_state(vm, &mut state.frame)
                    .map_err(|e| format!("Failed to create frame object: {:?}", e))?;
                
                // Wrap it in _FrameWrapper
                if let Ok(wrapper_class) = vm.builtins.get_attr("_FrameWrapper", vm) {
                    if let Ok(frame_obj) = vm.invoke(&wrapper_class, (frame_dict.clone(),)) {
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
                        let class_name = e.class().name().to_string();
                        let msg = vm.call_method(e.as_object(), "__str__", ())
                            .ok()
                            .and_then(|result| result.str(vm).ok().map(|s| s.to_string()))
                            .unwrap_or_default();
                        
                        if msg.is_empty() {
                            Err(format!("Python setup error: {}", class_name))
                        } else {
                            Err(format!("Python setup error: {}: {}", class_name, msg))
                        }
                    }
                }
            })
        } else {
            Err("No Python app instance".to_string())
        }
    }

    fn tick(&mut self, state: &mut EngineState) {
        if let Some(ref app_instance) = self.app_instance {
            self.interpreter.enter(|vm| {
                // Update frame data before calling tick
                if let Ok(Some(frame_obj)) = vm.get_attribute_opt(app_instance.clone(), "frame") {
                    let _ = crate::python::engine::py_bindings::update_py_frame_state(vm, frame_obj.clone(), &mut state.frame);
                    
                    // Print the frame array for debugging
                    if let Ok(Some(array_obj)) = vm.get_attribute_opt(frame_obj.clone(), "array") {
                        if let Ok(array_str) = vm.call_method(&array_obj, "__str__", ()) {
                            if let Ok(s) = array_str.str(vm) {
                                println!("[frame.array] {}", s.to_string());
                            }
                        }
                    }
                    
                    // Update mouse data
                    let mouse_dict = vm.ctx.new_dict();
                    let _ = mouse_dict.set_item("x", vm.ctx.new_float(state.mouse.x as f64).into(), vm);
                    let _ = mouse_dict.set_item("y", vm.ctx.new_float(state.mouse.y as f64).into(), vm);
                    let _ = mouse_dict.set_item("is_left_clicking", vm.ctx.new_bool(state.mouse.is_left_clicking).into(), vm);
                    let _ = app_instance.set_attr("mouse", mouse_dict, vm);
                    
                    // Call tick
                    if let Err(e) = vm.call_method(app_instance, "tick", ()) {
                        let class_name = e.class().name().to_string();
                        let msg = vm.call_method(e.as_object(), "__str__", ())
                            .ok()
                            .and_then(|result| result.str(vm).ok().map(|s| s.to_string()))
                            .unwrap_or_default();
                        
                        if !msg.is_empty() {
                            eprintln!("Python tick error: {}: {}", class_name, msg);
                        } else {
                            eprintln!("Python tick error: {}", class_name);
                        }
                    }
                    
                    // Sync Python buffer changes back to Rust
                    let _ = crate::python::engine::py_bindings::sync_py_buffer_to_rust(vm, frame_obj, &mut state.frame);
                }
            });
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
}
