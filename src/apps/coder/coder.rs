use crate::engine::{Application, EngineState};
use crate::apps::text::text::TextApp;
use crate::apps::coder::button::Button;

#[cfg(feature = "python")]
use rustpython_vm::{Interpreter, AsObject};

pub struct CoderApp {
    pub text_app: TextApp,
    #[cfg(feature = "python")]
    pub interpreter: Interpreter,
    pub run_button: Button,
}

impl CoderApp {
    pub fn new() -> Self {
        // Create the text app
        let text_app = TextApp::new();

        // Initialize RustPython interpreter with xos module
        #[cfg(feature = "python")]
        let interpreter = Interpreter::with_init(Default::default(), |vm| {
            // Register the xos native module
            vm.add_native_module("xos".to_owned(), Box::new(crate::python::xos_module::make_module));
        });

        // Create run button (position will be updated in tick)
        let run_button = Button::new(0, 0, 80, 30, "Run".to_string());

        Self {
            text_app,
            #[cfg(feature = "python")]
            interpreter,
            run_button,
        }
    }

    #[cfg(feature = "python")]
    fn execute_python_code(&mut self, code: &str) {
        println!("\n=== Executing Python Code ===");
        println!("{}", code);
        println!("--- Output ---");
        
        let result = self.interpreter.enter(|vm| {
            let scope = vm.new_scope_with_builtins();
            
            // Set __name__ to "__main__" so if __name__ == "__main__" works
            let _ = scope.globals.set_item("__name__", vm.ctx.new_str("__main__").into(), vm);
            
            // Run the code
            let exec_result = vm.run_code_string(scope.clone(), code, "<coder>".to_string());
            
            // Handle errors with detailed messages
            if let Err(py_exc) = exec_result {
                let class_name = py_exc.class().name();
                let error_msg = vm.call_method(py_exc.as_object(), "__str__", ())
                    .ok()
                    .and_then(|result| result.str(vm).ok())
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                
                if !error_msg.is_empty() {
                    eprintln!("Python Error: {}: {}", class_name, error_msg);
                } else {
                    eprintln!("Python Error: {}", class_name);
                }
                return Err(format!("{}: {}", class_name, error_msg));
            }
            
            // Check if an xos.Application was registered
            if let Ok(Some(_app_instance_obj)) = vm.get_attribute_opt(vm.builtins.as_object().to_owned(), "__xos_app_instance__") {
                println!("\n[xos] Application instance registered in coder!");
                println!("[xos] Note: The coder app cannot launch the xos engine window.");
                println!("[xos] To run this application with a window, save it to a file and run:");
                println!("[xos]   xos python <filename>.py");
            }
            
            Ok(())
        });

        match result {
            Ok(_) => {
                println!("--- Execution Complete ---\n");
            }
            Err(e) => {
                println!("--- Execution Failed ---");
                println!("{}\n", e);
            }
        }
    }

    #[cfg(not(feature = "python"))]
    fn execute_python_code(&mut self, _code: &str) {
        println!("\n=== Python execution not available (python feature disabled) ===\n");
    }

}

impl Application for CoderApp {
    fn setup(&mut self, state: &mut EngineState) -> Result<(), String> {
        // Delegate to text app
        self.text_app.setup(state)
    }

    fn tick(&mut self, state: &mut EngineState) {
        // Get dimensions before mutable borrow
        let shape = state.frame.array.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        let mouse_x = state.mouse.x;
        let mouse_y = state.mouse.y;
        
        // Delegate to text app
        self.text_app.tick(state);
        
        // Get buffer again for drawing button on top
        let buffer = state.frame_buffer_mut();
        
        // Update button position (bottom right)
        let padding = 10;
        self.run_button.x = (width as i32) - (self.run_button.width as i32) - padding;
        self.run_button.y = (height as i32) - (self.run_button.height as i32) - padding;
        
        // Check if mouse is hovering over button
        let is_hovered = self.run_button.contains_point(mouse_x, mouse_y);
        
        // Draw run button on top of everything
        self.run_button.draw(buffer, width as u32, height as u32, is_hovered);
    }

    fn on_scroll(&mut self, state: &mut EngineState, dx: f32, dy: f32) {
        self.text_app.on_scroll(state, dx, dy);
    }

    fn on_key_char(&mut self, state: &mut EngineState, ch: char) {
        self.text_app.on_key_char(state, ch);
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        self.text_app.on_mouse_move(state);
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        let mouse_x = state.mouse.x;
        let mouse_y = state.mouse.y;
        
        // Check if click is on the run button first
        if self.run_button.contains_point(mouse_x, mouse_y) {
            // Execute the Python code
            let code = self.text_app.text_engine.text.clone();
            if !code.trim().is_empty() {
                self.execute_python_code(&code);
            }
            return;
        }
        
        // Otherwise delegate to text app
        self.text_app.on_mouse_down(state);
    }

    fn on_mouse_up(&mut self, state: &mut EngineState) {
        self.text_app.on_mouse_up(state);
    }
}

