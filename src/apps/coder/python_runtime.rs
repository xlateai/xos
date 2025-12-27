//! Python runtime execution module for running Python code in background threads and viewport apps

use rustpython_vm::{Interpreter, AsObject};
use std::sync::{Arc, Mutex};
use std::thread;

pub struct PythonRuntime {
    pub interpreter: Interpreter,
    pub persistent_scope: Option<rustpython_vm::scope::Scope>,
    // Viewport app instance
    pub viewport_app: Option<rustpython_vm::PyObjectRef>,
    pub viewport_app_setup_done: bool,
    // Background execution
    pub output_buffer: Arc<Mutex<String>>,
    pub thread_handle: Option<thread::JoinHandle<()>>,
    pub thread_running: Arc<Mutex<bool>>,
    pub thread_generation: Arc<Mutex<u64>>,
}

impl PythonRuntime {
    pub fn new() -> Self {
        // Initialize RustPython interpreter with xos module
        let interpreter = Interpreter::with_init(Default::default(), |vm| {
            // Register the xos native module
            vm.add_native_module("xos".to_owned(), Box::new(crate::python::xos_module::make_module));
        });
        
        Self {
            interpreter,
            persistent_scope: None,
            viewport_app: None,
            viewport_app_setup_done: false,
            output_buffer: Arc::new(Mutex::new(String::new())),
            thread_handle: None,
            thread_running: Arc::new(Mutex::new(false)),
            thread_generation: Arc::new(Mutex::new(0)),
        }
    }
    
    pub fn stop_all(&mut self) {
        // Wait for any previous thread to complete
        if let Some(handle) = self.thread_handle.take() {
            *self.thread_running.lock().unwrap() = false;
            let _ = handle.join();
        }
        
        // Clear viewport app
        self.viewport_app = None;
        self.viewport_app_setup_done = false;
        
        // Clean up audio resources
        crate::python::audio::cleanup_all_audio();
    }
    
    pub fn execute_code(&mut self, code: &str) -> ExecutionResult {
        // Stop any previous execution
        self.stop_all();
        
        // Clear output buffer
        {
            let mut buffer = self.output_buffer.lock().unwrap();
            buffer.clear();
        }
        
        // Detect if this is a viewport app
        let is_viewport_app = code.contains("xos.Application") || (code.contains("class") && code.contains("Application"));
        
        if is_viewport_app {
            self.execute_viewport_app(code)
        } else {
            self.execute_background_script(code);
            ExecutionResult::BackgroundStarted
        }
    }
    
    fn execute_viewport_app(&mut self, code: &str) -> ExecutionResult {
        let result = self.interpreter.enter(|vm| {
            // Clear the previous app instance from builtins
            let _ = vm.builtins.as_object().to_owned().del_attr("__xos_app_instance__", vm);
            
            // Get or create persistent scope
            let scope = if let Some(ref existing_scope) = self.persistent_scope {
                existing_scope.clone()
            } else {
                let new_scope = vm.new_scope_with_builtins();
                let _ = new_scope.globals.set_item("__name__", vm.ctx.new_str("__main__").into(), vm);
                self.persistent_scope = Some(new_scope.clone());
                new_scope
            };
            
            // Run the code
            let exec_result = vm.run_code_string(scope.clone(), code, "<coder>".to_string());
            
            // Handle errors
            if let Err(py_exc) = exec_result {
                let class_name = py_exc.class().name();
                let error_msg = vm.call_method(py_exc.as_object(), "__str__", ())
                    .ok()
                    .and_then(|result| result.str(vm).ok())
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                
                let error_text = if !error_msg.is_empty() {
                    format!("{}: {}", class_name, error_msg)
                } else {
                    format!("{}", class_name)
                };
                
                return Err(error_text);
            }
            
            // Check if an xos.Application was registered
            if let Ok(Some(app_instance_obj)) = vm.get_attribute_opt(vm.builtins.as_object().to_owned(), "__xos_app_instance__") {
                self.viewport_app = Some(app_instance_obj);
                self.viewport_app_setup_done = false;
                Ok("[xos] Application registered - rendering to viewport tab\n".to_string())
            } else {
                Ok("(no output)".to_string())
            }
        });
        
        match result {
            Ok(msg) => ExecutionResult::ViewportSuccess(msg),
            Err(error) => ExecutionResult::Error(error + "\n"),
        }
    }
    
    fn execute_background_script(&mut self, code: &str) {
        let code_str = code.to_string();
        let output_buffer = Arc::clone(&self.output_buffer);
        let running_flag = Arc::clone(&self.thread_running);
        let generation_counter = Arc::clone(&self.thread_generation);
        
        // Increment generation (invalidates any previous thread's output)
        let current_generation = {
            let mut gen = self.thread_generation.lock().unwrap();
            *gen += 1;
            *gen
        };
        
        // Mark thread as running
        *self.thread_running.lock().unwrap() = true;
        
        // Spawn background thread to execute Python
        let handle = thread::spawn(move || {
            let interpreter = Interpreter::with_init(Default::default(), |vm| {
                vm.add_native_module("xos".to_owned(), Box::new(crate::python::xos_module::make_module));
            });
            
            interpreter.enter(|vm| {
                let scope = vm.new_scope_with_builtins();
                let _ = scope.globals.set_item("__name__", vm.ctx.new_str("__main__").into(), vm);
            
                // Create output capture function
                let buffer_clone = Arc::clone(&output_buffer);
                let generation_clone = Arc::clone(&generation_counter);
                let write_output_fn = vm.new_function(
                    "__write_output__",
                    move |args: rustpython_vm::function::FuncArgs, _vm: &rustpython_vm::VirtualMachine| -> rustpython_vm::PyResult {
                        if let Ok(current_gen) = generation_clone.lock() {
                            if *current_gen == current_generation {
                                if let Some(text_obj) = args.args.first() {
                                    if let Ok(text) = text_obj.str(_vm) {
                                        if let Ok(mut buffer) = buffer_clone.lock() {
                                            buffer.push_str(&text.to_string());
                                        }
                                    }
                                }
                            }
                        }
                        Ok(_vm.ctx.none())
                    },
                );
                scope.globals.set_item("__write_output__", write_output_fn.into(), vm).ok();
                
                // Override print
                let setup_code = r#"
import builtins
__original_print__ = builtins.print

def __custom_print__(*args, sep=' ', end='\n', **kwargs):
    output = sep.join(str(arg) for arg in args) + end
    __write_output__(output)

builtins.print = __custom_print__
"#;
                
                if let Err(e) = vm.run_code_string(scope.clone(), setup_code, "<setup>".to_string()) {
                    eprintln!("Failed to set up print capture: {:?}", e);
                }
                
                // Run the user's code
                let exec_result = vm.run_code_string(scope.clone(), &code_str, "<coder>".to_string());
                
                // Restore original print
                let restore_code = "builtins.print = __original_print__";
                vm.run_code_string(scope.clone(), restore_code, "<restore>".to_string()).ok();
            
                // Handle errors
                if let Err(py_exc) = exec_result {
                    if let Ok(current_gen) = generation_counter.lock() {
                        if *current_gen == current_generation {
                            let class_name = py_exc.class().name();
                            let error_msg = vm.call_method(py_exc.as_object(), "__str__", ())
                                .ok()
                                .and_then(|result| result.str(vm).ok())
                                .map(|s| s.to_string())
                                .unwrap_or_default();
                            
                            let error_text = if !error_msg.is_empty() {
                                format!("\n{}: {}\n", class_name, error_msg)
                            } else {
                                format!("\n{}\n", class_name)
                            };
                            
                            if let Ok(mut buffer) = output_buffer.lock() {
                                buffer.push_str(&error_text);
                            }
                        }
                    }
                } else {
                    if let Ok(current_gen) = generation_counter.lock() {
                        if *current_gen == current_generation {
                            if let Ok(Some(_)) = vm.get_attribute_opt(vm.builtins.as_object().to_owned(), "__xos_app_instance__") {
                                if let Ok(mut buffer) = output_buffer.lock() {
                                    buffer.push_str("\n[xos] Application registered - switch to viewport tab\n");
                                }
                            } else {
                                if let Ok(buffer) = output_buffer.lock() {
                                    if buffer.trim().is_empty() {
                                        drop(buffer);
                                        if let Ok(mut buffer) = output_buffer.lock() {
                                            buffer.push_str("(no output)\n");
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            });
            
            // Mark thread as no longer running
            if let Ok(current_gen) = generation_counter.lock() {
                if *current_gen == current_generation {
                    if let Ok(mut flag) = running_flag.lock() {
                        *flag = false;
                    }
                }
            }
        });
        
        self.thread_handle = Some(handle);
    }
    
    pub fn execute_console_command(&mut self, command: &str) -> Result<(String, bool), (String, bool)> {
        let lines: Vec<&str> = command.split('\n').collect();
        let current_line = lines.last().unwrap_or(&"").trim();
        
        if current_line.is_empty() {
            return Ok((String::new(), false));
        }
        
        let actual_command = current_line;
        
        self.interpreter.enter(|vm| {
            // Get or create persistent scope
            let scope = if let Some(ref existing_scope) = self.persistent_scope {
                existing_scope.clone()
            } else {
                let new_scope = vm.new_scope_with_builtins();
                let _ = new_scope.globals.set_item("__name__", vm.ctx.new_str("__main__").into(), vm);
                self.persistent_scope = Some(new_scope.clone());
                new_scope
            };
            
            // Store output buffer reference
            let output_list = vm.ctx.new_list(vec![]);
            scope.globals.set_item("__output_lines__", output_list.clone().into(), vm).ok();
            
            // Override print
            let setup_code = r#"
import builtins
__original_print__ = builtins.print

def __custom_print__(*args, sep=' ', end='\n', **kwargs):
    output = sep.join(str(arg) for arg in args) + end
    __output_lines__.append(output)

builtins.print = __custom_print__
"#;
            
            if let Err(e) = vm.run_code_string(scope.clone(), setup_code, "<setup>".to_string()) {
                eprintln!("Failed to set up print capture: {:?}", e);
            }
            
            // Try eval first, then exec
            let eval_code = format!("__console_result = eval({:?})", actual_command);
            let eval_result = vm.run_code_string(scope.clone(), &eval_code, "<console>".to_string());
            
            // Extract captured output
            let captured_output = if let Ok(output_obj) = scope.globals.get_item("__output_lines__", vm) {
                if let Ok(output_list) = output_obj.downcast::<rustpython_vm::builtins::PyList>() {
                    let mut result = String::new();
                    for item in output_list.borrow_vec().iter() {
                        if let Ok(s) = item.str(vm) {
                            result.push_str(&s.to_string());
                        }
                    }
                    result
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            
            // Restore original print
            let restore_code = "builtins.print = __original_print__";
            vm.run_code_string(scope.clone(), restore_code, "<restore>".to_string()).ok();
            
            if eval_result.is_ok() {
                if let Ok(result_obj) = scope.globals.get_item("__console_result", vm) {
                    if !vm.is_none(&result_obj) {
                        if let Ok(repr_str) = vm.call_method(&result_obj, "__repr__", ()) {
                            if let Ok(s) = repr_str.str(vm) {
                                return Ok((captured_output + &s.to_string(), false));
                            }
                        }
                    }
                }
                Ok((captured_output, false))
            } else {
                let exec_result = vm.run_code_string(scope.clone(), actual_command, "<console>".to_string());
                
                if let Err(py_exc) = exec_result {
                    let class_name = py_exc.class().name();
                    let error_msg = vm.call_method(py_exc.as_object(), "__str__", ())
                        .ok()
                        .and_then(|result| result.str(vm).ok())
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    
                    let error_text = if !error_msg.is_empty() {
                        format!("{}: {}", class_name, error_msg)
                    } else {
                        format!("{}", class_name)
                    };
                    
                    return Err((captured_output + &error_text, true));
                }
                
                Ok((captured_output, false))
            }
        })
    }
    
    pub fn check_thread_finished(&mut self) -> bool {
        if let Some(handle) = &self.thread_handle {
            if handle.is_finished() {
                if let Some(handle) = self.thread_handle.take() {
                    let _ = handle.join();
                }
                return true;
            }
        }
        false
    }
    
    pub fn get_output(&self) -> Option<String> {
        self.output_buffer.try_lock().ok().map(|buffer| buffer.clone())
    }
    
    pub fn is_thread_running(&self) -> bool {
        self.thread_running.lock().map(|f| *f).unwrap_or(false)
    }
    
    pub fn is_viewport_app_running(&self) -> bool {
        self.viewport_app.is_some()
    }
    
    pub fn stop_execution(&mut self) -> String {
        let mut messages = Vec::new();
        
        // Stop viewport app
        if self.viewport_app.is_some() {
            self.viewport_app = None;
            self.viewport_app_setup_done = false;
            crate::python::audio::cleanup_all_audio();
            messages.push("[xos] Viewport app stopped");
        }
        
        // Stop background thread
        if self.is_thread_running() {
            // Increment generation counter
            if let Ok(mut gen) = self.thread_generation.lock() {
                *gen += 1;
            }
            
            // Mark as not running
            if let Ok(mut flag) = self.thread_running.lock() {
                *flag = false;
            }
            
            // Drop thread handle
            self.thread_handle = None;
            
            // Clean up audio
            crate::python::audio::cleanup_all_audio();
            
            messages.push("[xos] Script stopped by user");
        }
        
        messages.join("\n") + "\n"
    }
}

pub enum ExecutionResult {
    ViewportSuccess(String),
    BackgroundStarted,
    Error(String),
}

