#[cfg(feature = "python")]
use rustpython_vm::{Interpreter, PyObjectRef, VirtualMachine, AsObject};
use crate::engine::{Application, EngineState};

pub const APPLICATION_CLASS_CODE: &str = r#"
class Application:
    """Base class for xos applications. Extend this class and implement setup() and tick()."""
    
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
                // TODO: Pass state to Python setup method
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
                // TODO: Pass state to Python tick method
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
