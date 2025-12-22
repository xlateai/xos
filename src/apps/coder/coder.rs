use crate::engine::{Application, EngineState};
use crate::apps::text::text::TextApp;
use crate::apps::coder::button::Button;
use crate::text::text_rasterization::TextRasterizer;
use rustpython_vm::{Interpreter, AsObject};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Tab {
    Code,
    Terminal,
    Viewport,
}

#[derive(Debug, Clone)]
struct PythonFile {
    name: String,
    content: String,
    #[allow(dead_code)]
    path: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CodeViewMode {
    Editor,
    FileExplorer,
}

pub struct CoderApp {
    pub code_app: TextApp,
    pub terminal_app: TextApp,
    pub console_app: TextApp,
    active_tab: Tab,
    pub interpreter: Interpreter,
    pub run_button: Button,
    pub stop_button: Button,
    pub clear_button: Button,
    pub clear_button_label: TextRasterizer,
    pub code_tab_label: TextRasterizer,
    pub terminal_tab_label: TextRasterizer,
    pub viewport_tab_label: TextRasterizer,
    persistent_scope: Option<rustpython_vm::scope::Scope>,
    // Python app instance for viewport rendering
    viewport_app: Option<rustpython_vm::PyObjectRef>,
    viewport_app_setup_done: bool,
    // Viewport double-tap tracking
    viewport_last_tap_time: Option<std::time::Instant>,
    viewport_last_tap_x: f32,
    viewport_last_tap_y: f32,
    // File explorer
    code_view_mode: CodeViewMode,
    python_files: Vec<PythonFile>,
    current_file_index: usize,
    file_list_scroll_y: f32,
}

impl CoderApp {
    // Get button/tab dimensions based on platform
    fn get_button_size() -> (u32, u32) {
        #[cfg(target_os = "ios")]
        {
            // 75% bigger on iOS for better touch targets
            (280, 105)
        }
        #[cfg(not(target_os = "ios"))]
        {
            (160, 60)
        }
    }
    
    // Get tab width (narrower on iOS to make room for stop button)
    fn get_tab_width() -> u32 {
        let (button_width, _) = Self::get_button_size();
        #[cfg(target_os = "ios")]
        {
            // 20% narrower on iOS
            (button_width as f32 * 0.8) as u32
        }
        #[cfg(not(target_os = "ios"))]
        {
            button_width
        }
    }
    
    pub fn new() -> Self {
        // Load all Python files from the python/ directory
        let python_files = vec![
            PythonFile {
                name: "ball.py".to_string(),
                content: include_str!("../../../python/ball.py").to_string(),
                path: "ball.py".to_string(),
            },
            PythonFile {
                name: "hello.py".to_string(),
                content: include_str!("../../../python/hello.py").to_string(),
                path: "hello.py".to_string(),
            },
            PythonFile {
                name: "old/galaxy.py".to_string(),
                content: include_str!("../../../python/old/galaxy.py").to_string(),
                path: "old/galaxy.py".to_string(),
            },
            PythonFile {
                name: "old/python_camera.py".to_string(),
                content: include_str!("../../../python/old/python_camera.py").to_string(),
                path: "old/python_camera.py".to_string(),
            },
            PythonFile {
                name: "old/setup.py".to_string(),
                content: include_str!("../../../python/old/setup.py").to_string(),
                path: "old/setup.py".to_string(),
            },
        ];
        
        // Create the code editor text app with default code from ball.py
        let mut code_app = TextApp::new();
        code_app.text_rasterizer.text = python_files[0].content.clone();
        code_app.show_debug_visuals = false; // Hide debug visuals in code editor
        code_app.scroll_y = 0.0; // Start scroll at the top
        
        // Create the terminal text app (empty initially)
        let mut terminal_app = TextApp::new();
        terminal_app.text_rasterizer.text = "".to_string();
        terminal_app.read_only = true; // Terminal is read-only
        terminal_app.show_cursor = false; // Hide cursor in terminal
        terminal_app.show_debug_visuals = false; // Hide debug visuals in terminal
        
        // Create the console text app for interactive Python
        let mut console_app = TextApp::new();
        console_app.text_rasterizer.text = "".to_string();
        console_app.show_debug_visuals = false; // Hide debug visuals in console

        // Initialize RustPython interpreter with xos module
        let interpreter = Interpreter::with_init(Default::default(), |vm| {
            // Register the xos native module
            vm.add_native_module("xos".to_owned(), Box::new(crate::python::xos_module::make_module));
        });

        // Initialize persistent scope for console
        let persistent_scope = None; // Will be created on first use

        // Create run button (position will be updated in tick)
        let (button_width, button_height) = Self::get_button_size();
        let run_button = Button::new(0, 0, button_width, button_height, "Run".to_string());
        
        // Create stop button (smaller, appears during execution)
        let stop_button_width = button_height; // Square button
        let mut stop_button = Button::new(0, 0, stop_button_width, button_height, "Stop".to_string());
        stop_button.bg_color = (200, 50, 50); // Red
        stop_button.hover_color = (220, 70, 70); // Lighter red on hover
        
        // Create clear button (smaller, will be positioned in console area)
        let clear_button_size = (button_height, button_height); // Square button
        let mut clear_button = Button::new(0, 0, clear_button_size.0, clear_button_size.1, "X".to_string());
        clear_button.bg_color = (80, 80, 80); // Gray
        clear_button.hover_color = (100, 100, 100); // Lighter gray on hover
        
        // Load font for tab labels
        let font_data = include_bytes!("../../../assets/JetBrainsMono-Regular.ttf");
        let font = fontdue::Font::from_bytes(
            font_data as &[u8],
            fontdue::FontSettings::default(),
        ).expect("Failed to load font");
        
        // Create text rasterizers for tab labels
        let mut code_tab_label = TextRasterizer::new(font.clone(), 20.0);
        code_tab_label.set_text("code.py".to_string());
        
        let mut terminal_tab_label = TextRasterizer::new(font.clone(), 20.0);
        terminal_tab_label.set_text("terminal".to_string());
        
        let mut viewport_tab_label = TextRasterizer::new(font.clone(), 20.0);
        viewport_tab_label.set_text("viewport".to_string());
        
        // Create text rasterizer for clear button "x" label
        let mut clear_button_label = TextRasterizer::new(font, 30.0);
        clear_button_label.set_text("×".to_string()); // Multiplication sign

        Self {
            code_app,
            terminal_app,
            console_app,
            active_tab: Tab::Code,
            interpreter,
            run_button,
            stop_button,
            clear_button,
            clear_button_label,
            code_tab_label,
            terminal_tab_label,
            viewport_tab_label,
            persistent_scope,
            viewport_app: None,
            viewport_app_setup_done: false,
            viewport_last_tap_time: None,
            viewport_last_tap_x: 0.0,
            viewport_last_tap_y: 0.0,
            code_view_mode: CodeViewMode::Editor,
            python_files,
            current_file_index: 0,
            file_list_scroll_y: 0.0,
        }
    }

    fn execute_python_code(&mut self, code: &str) {
        // Clear terminal - just show the raw output
        self.terminal_app.text_rasterizer.text.clear();
        
        let result = self.interpreter.enter(|vm| {
            // Get or create persistent scope
            let scope = if let Some(ref existing_scope) = self.persistent_scope {
                existing_scope.clone()
            } else {
                let new_scope = vm.new_scope_with_builtins();
                // Set __name__ to "__main__" so if __name__ == "__main__" works
                let _ = new_scope.globals.set_item("__name__", vm.ctx.new_str("__main__").into(), vm);
                self.persistent_scope = Some(new_scope.clone());
                new_scope
            };
            
            // Store output buffer reference in scope so we can access it
            // We'll use a simple Python list to capture output
            let output_list = vm.ctx.new_list(vec![]);
            scope.globals.set_item("__output_lines__", output_list.clone().into(), vm).ok();
            
            // Override print in builtins to capture output
            let setup_code = r#"
import builtins
__original_print__ = builtins.print

def __custom_print__(*args, sep=' ', end='\n', **kwargs):
    output = sep.join(str(arg) for arg in args) + end
    __output_lines__.append(output)

builtins.print = __custom_print__
"#;
            
            // Set up print capture
            if let Err(e) = vm.run_code_string(scope.clone(), setup_code, "<setup>".to_string()) {
                eprintln!("Failed to set up print capture: {:?}", e);
            }
            
            // Run the user's code
            let exec_result = vm.run_code_string(scope.clone(), code, "<coder>".to_string());
            
            // Restore original print
            let restore_code = "builtins.print = __original_print__";
            vm.run_code_string(scope.clone(), restore_code, "<restore>".to_string()).ok();
            
            // Extract the captured output from the list
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
            
            // Handle errors with detailed messages
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
                
                return Err((error_text, captured_output));
            }
            
            // Check if an xos.Application was registered
            let mut extra_output = String::new();
            let app_registered = if let Ok(Some(app_instance_obj)) = vm.get_attribute_opt(vm.builtins.as_object().to_owned(), "__xos_app_instance__") {
                extra_output.push_str("[xos] Application registered - rendering to viewport tab\n");
                
                // Store the app instance for viewport rendering
                self.viewport_app = Some(app_instance_obj);
                self.viewport_app_setup_done = false;
                
                true
            } else {
                false
            };
            
            Ok((captured_output, extra_output, app_registered))
        });

        // Display output in terminal
        let should_show_viewport = match result {
            Ok((output, extra, app_registered)) => {
                self.terminal_app.text_rasterizer.text = output + &extra;
                if self.terminal_app.text_rasterizer.text.trim().is_empty() {
                    self.terminal_app.text_rasterizer.text = "(no output)".to_string();
                }
                app_registered
            }
            Err((error, output)) => {
                self.terminal_app.text_rasterizer.text = output + &error + "\n";
                false
            }
        };
        
        // Smart tab switching: only switch if currently on Code tab
        if self.active_tab == Tab::Code {
            if should_show_viewport {
                // App registered - switch to viewport to show the running app
                self.active_tab = Tab::Viewport;
            } else {
                // No app - switch to terminal to show output
                self.active_tab = Tab::Terminal;
            }
        }
        // If already on Terminal or Viewport, stay there
    }

    fn execute_console_command(&mut self, command: &str) {
        // Get the current line (after the last newline)
        let lines: Vec<&str> = command.split('\n').collect();
        let current_line = lines.last().unwrap_or(&"").trim();
        
        if current_line.is_empty() {
            return;
        }
        
        let actual_command = current_line;
        
        // Execute in the same scope as run code
        let result = self.interpreter.enter(|vm| {
            // Get or create persistent scope
            let scope = if let Some(ref existing_scope) = self.persistent_scope {
                existing_scope.clone()
            } else {
                let new_scope = vm.new_scope_with_builtins();
                let _ = new_scope.globals.set_item("__name__", vm.ctx.new_str("__main__").into(), vm);
                self.persistent_scope = Some(new_scope.clone());
                new_scope
            };
            
            // Store output buffer reference in scope so we can capture print statements
            let output_list = vm.ctx.new_list(vec![]);
            scope.globals.set_item("__output_lines__", output_list.clone().into(), vm).ok();
            
            // Override print in builtins to capture output
            let setup_code = r#"
import builtins
__original_print__ = builtins.print

def __custom_print__(*args, sep=' ', end='\n', **kwargs):
    output = sep.join(str(arg) for arg in args) + end
    __output_lines__.append(output)

builtins.print = __custom_print__
"#;
            
            // Set up print capture
            if let Err(e) = vm.run_code_string(scope.clone(), setup_code, "<setup>".to_string()) {
                eprintln!("Failed to set up print capture: {:?}", e);
            }
            
            // Try eval first (for expressions), then exec
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
                // Check if result is None
                if let Ok(result_obj) = scope.globals.get_item("__console_result", vm) {
                    if !vm.is_none(&result_obj) {
                        // Print the result
                        if let Ok(repr_str) = vm.call_method(&result_obj, "__repr__", ()) {
                            if let Ok(s) = repr_str.str(vm) {
                                return Ok((captured_output + &s.to_string(), false));
                            }
                        }
                    }
                }
                Ok((captured_output, false))
            } else {
                // Eval failed, try exec
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
        });
        
        // Append command and result to terminal
        if !self.terminal_app.text_rasterizer.text.is_empty() {
            self.terminal_app.text_rasterizer.text.push_str("\n");
        }
        self.terminal_app.text_rasterizer.text.push_str(">>> ");
        self.terminal_app.text_rasterizer.text.push_str(actual_command);
        self.terminal_app.text_rasterizer.text.push_str("\n");
        
        match result {
            Ok((output, _)) => {
                if !output.is_empty() {
                    self.terminal_app.text_rasterizer.text.push_str(&output);
                }
            }
            Err((error, _)) => {
                self.terminal_app.text_rasterizer.text.push_str(&error);
                self.terminal_app.text_rasterizer.text.push_str("\n");
            }
        }
    }

    fn draw_tab(&self, buffer: &mut [u8], canvas_width: u32, canvas_height: u32, x: i32, y: i32, width: u32, height: u32, label_rasterizer: &TextRasterizer, is_active: bool) {
        let bg_color = if is_active { (60, 60, 60) } else { (40, 40, 40) };
        let text_color = if is_active { (255, 255, 255) } else { (150, 150, 150) };
        
        // Draw tab background
        for dy in 0..height {
            for dx in 0..width {
                let px = x + dx as i32;
                let py = y + dy as i32;
                
                if px >= 0 && px < canvas_width as i32 && py >= 0 && py < canvas_height as i32 {
                    let idx = ((py as u32 * canvas_width + px as u32) * 4) as usize;
                    buffer[idx + 0] = bg_color.0;
                    buffer[idx + 1] = bg_color.1;
                    buffer[idx + 2] = bg_color.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }
        
        // Draw tab border
        // Top border
        for dx in 0..width {
            let px = x + dx as i32;
            if px >= 0 && px < canvas_width as i32 && y >= 0 && y < canvas_height as i32 {
                let idx = ((y as u32 * canvas_width + px as u32) * 4) as usize;
                buffer[idx + 0] = text_color.0;
                buffer[idx + 1] = text_color.1;
                buffer[idx + 2] = text_color.2;
                buffer[idx + 3] = 0xff;
            }
        }
        // Bottom border (only if not active)
        if !is_active {
            let bottom_y = y + height as i32 - 1;
            for dx in 0..width {
                let px = x + dx as i32;
                if px >= 0 && px < canvas_width as i32 && bottom_y >= 0 && bottom_y < canvas_height as i32 {
                    let idx = ((bottom_y as u32 * canvas_width + px as u32) * 4) as usize;
                    buffer[idx + 0] = text_color.0;
                    buffer[idx + 1] = text_color.1;
                    buffer[idx + 2] = text_color.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }
        // Left border
        for dy in 0..height {
            let py = y + dy as i32;
            if py >= 0 && py < canvas_height as i32 && x >= 0 && x < canvas_width as i32 {
                let idx = ((py as u32 * canvas_width + x as u32) * 4) as usize;
                buffer[idx + 0] = text_color.0;
                buffer[idx + 1] = text_color.1;
                buffer[idx + 2] = text_color.2;
                buffer[idx + 3] = 0xff;
            }
        }
        // Right border
        let right_x = x + width as i32 - 1;
        for dy in 0..height {
            let py = y + dy as i32;
            if py >= 0 && py < canvas_height as i32 && right_x >= 0 && right_x < canvas_width as i32 {
                let idx = ((py as u32 * canvas_width + right_x as u32) * 4) as usize;
                buffer[idx + 0] = text_color.0;
                buffer[idx + 1] = text_color.1;
                buffer[idx + 2] = text_color.2;
                buffer[idx + 3] = 0xff;
            }
        }
        
        // Draw label text centered in the tab
        for character in &label_rasterizer.characters {
            // Center the text horizontally in the tab
            let text_width = label_rasterizer.characters.iter()
                .map(|c| c.metrics.advance_width)
                .sum::<f32>();
            let text_offset_x = (width as f32 - text_width) / 2.0;
            let text_offset_y = (height as f32 - label_rasterizer.font_size) / 2.0;
            
            let char_x = x as f32 + character.x + text_offset_x;
            let char_y = y as f32 + character.y + text_offset_y;
            
            for (bitmap_y, row) in character.bitmap.chunks(character.width as usize).enumerate() {
                for (bitmap_x, &alpha) in row.iter().enumerate() {
                    if alpha == 0 {
                        continue;
                    }
                    
                    let px = (char_x + bitmap_x as f32) as i32;
                    let py = (char_y + bitmap_y as f32) as i32;
                    
                    if px >= 0 && px < canvas_width as i32 && py >= 0 && py < canvas_height as i32 {
                        let idx = ((py as u32 * canvas_width + px as u32) * 4) as usize;
                        
                        // Blend text color with alpha
                        let alpha_f = alpha as f32 / 255.0;
                        buffer[idx + 0] = ((text_color.0 as f32 * alpha_f) + (buffer[idx + 0] as f32 * (1.0 - alpha_f))) as u8;
                        buffer[idx + 1] = ((text_color.1 as f32 * alpha_f) + (buffer[idx + 1] as f32 * (1.0 - alpha_f))) as u8;
                        buffer[idx + 2] = ((text_color.2 as f32 * alpha_f) + (buffer[idx + 2] as f32 * (1.0 - alpha_f))) as u8;
                    }
                }
            }
        }
    }

    fn tab_contains_point(&self, x: f32, y: f32, tab_x: i32, tab_y: i32, tab_width: u32, tab_height: u32) -> bool {
        x >= tab_x as f32 
            && x < (tab_x + tab_width as i32) as f32
            && y >= tab_y as f32
            && y < (tab_y + tab_height as i32) as f32
    }

    fn load_file(&mut self, file_index: usize) {
        if file_index < self.python_files.len() {
            self.current_file_index = file_index;
            self.code_app.text_rasterizer.text = self.python_files[file_index].content.clone();
            self.code_app.cursor_position = 0;
            self.code_app.scroll_y = 0.0; // Start at top when loading a file
            self.code_view_mode = CodeViewMode::Editor;
            
            // Update the tab label to show the current file name
            self.code_tab_label.set_text(self.python_files[file_index].name.clone());
        }
    }
    
    fn save_current_file(&mut self) {
        // Save the current editor content back to the file in memory
        if self.current_file_index < self.python_files.len() {
            self.python_files[self.current_file_index].content = self.code_app.text_rasterizer.text.clone();
        }
    }
    
    fn draw_file_explorer(&self, buffer: &mut [u8], canvas_width: u32, canvas_height: u32, viewport_height: f32) {
        // Draw file list background
        let bg_color = (20, 20, 20);
        for y in 0..(viewport_height as i32) {
            if y >= 0 && y < canvas_height as i32 {
                for x in 0..(canvas_width as i32) {
                    let idx = ((y as u32 * canvas_width + x as u32) * 4) as usize;
                    buffer[idx + 0] = bg_color.0;
                    buffer[idx + 1] = bg_color.1;
                    buffer[idx + 2] = bg_color.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }
        
        // Draw file list
        let font_data = include_bytes!("../../../assets/JetBrainsMono-Regular.ttf");
        let font = fontdue::Font::from_bytes(
            font_data as &[u8],
            fontdue::FontSettings::default(),
        ).expect("Failed to load font");
        
        let item_height = 60.0;
        let padding = 10.0;
        let text_size = 24.0;
        
        for (i, file) in self.python_files.iter().enumerate() {
            let y_offset = i as f32 * item_height - self.file_list_scroll_y;
            
            // Skip if not visible
            if y_offset + item_height < 0.0 || y_offset > viewport_height {
                continue;
            }
            
            // Draw item background (highlight if current file)
            let item_bg_color = if i == self.current_file_index {
                (40, 60, 80) // Highlight current file
            } else {
                (30, 30, 30)
            };
            
            for dy in 0..(item_height as i32) {
                let y = y_offset as i32 + dy;
                if y >= 0 && y < viewport_height as i32 {
                    for x in 0..(canvas_width as i32) {
                        let idx = ((y as u32 * canvas_width + x as u32) * 4) as usize;
                        buffer[idx + 0] = item_bg_color.0;
                        buffer[idx + 1] = item_bg_color.1;
                        buffer[idx + 2] = item_bg_color.2;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
            
            // Draw file name text
            let text_color = (200, 200, 200);
            let mut text_x = padding;
            let text_y = y_offset + (item_height - text_size) / 2.0;
            
            for ch in file.name.chars() {
                let (metrics, bitmap) = font.rasterize(ch, text_size);
                
                for (bitmap_y, row) in bitmap.chunks(metrics.width).enumerate() {
                    for (bitmap_x, &alpha) in row.iter().enumerate() {
                        if alpha == 0 {
                            continue;
                        }
                        
                        let px = (text_x + bitmap_x as f32) as i32;
                        let py = (text_y + bitmap_y as f32) as i32;
                        
                        if px >= 0 && px < canvas_width as i32 && py >= 0 && py < viewport_height as i32 {
                            let idx = ((py as u32 * canvas_width + px as u32) * 4) as usize;
                            
                            // Blend text color with alpha
                            let alpha_f = alpha as f32 / 255.0;
                            buffer[idx + 0] = ((text_color.0 as f32 * alpha_f) + (buffer[idx + 0] as f32 * (1.0 - alpha_f))) as u8;
                            buffer[idx + 1] = ((text_color.1 as f32 * alpha_f) + (buffer[idx + 1] as f32 * (1.0 - alpha_f))) as u8;
                            buffer[idx + 2] = ((text_color.2 as f32 * alpha_f) + (buffer[idx + 2] as f32 * (1.0 - alpha_f))) as u8;
                        }
                    }
                }
                
                text_x += metrics.advance_width;
            }
            
            // Draw separator line
            let separator_y = (y_offset + item_height - 1.0) as i32;
            if separator_y >= 0 && separator_y < viewport_height as i32 {
                for x in 0..(canvas_width as i32) {
                    let idx = ((separator_y as u32 * canvas_width + x as u32) * 4) as usize;
                    buffer[idx + 0] = 50;
                    buffer[idx + 1] = 50;
                    buffer[idx + 2] = 50;
                    buffer[idx + 3] = 0xff;
                }
            }
        }
    }

    fn draw_button_with_color(&self, buffer: &mut [u8], canvas_width: u32, canvas_height: u32, is_hovered: bool, color: (u8, u8, u8)) {
        let x = self.run_button.x;
        let y = self.run_button.y;
        let width = self.run_button.width;
        let height = self.run_button.height;
        
        let bg_color = if is_hovered {
            // Slightly lighter when hovered
            (
                (color.0 as u16 * 120 / 100).min(255) as u8,
                (color.1 as u16 * 120 / 100).min(255) as u8,
                (color.2 as u16 * 120 / 100).min(255) as u8,
            )
        } else {
            color
        };
        
        // Draw button background
        for dy in 0..height {
            for dx in 0..width {
                let px = x + dx as i32;
                let py = y + dy as i32;
                
                if px >= 0 && px < canvas_width as i32 && py >= 0 && py < canvas_height as i32 {
                    let idx = ((py as u32 * canvas_width + px as u32) * 4) as usize;
                    buffer[idx + 0] = bg_color.0;
                    buffer[idx + 1] = bg_color.1;
                    buffer[idx + 2] = bg_color.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }
        
        // Draw button border
        let border_color = (255, 255, 255);
        // Top border
        for dx in 0..width {
            let px = x + dx as i32;
            if px >= 0 && px < canvas_width as i32 && y >= 0 && y < canvas_height as i32 {
                let idx = ((y as u32 * canvas_width + px as u32) * 4) as usize;
                buffer[idx + 0] = border_color.0;
                buffer[idx + 1] = border_color.1;
                buffer[idx + 2] = border_color.2;
                buffer[idx + 3] = 0xff;
            }
        }
        // Bottom border
        let bottom_y = y + height as i32 - 1;
        for dx in 0..width {
            let px = x + dx as i32;
            if px >= 0 && px < canvas_width as i32 && bottom_y >= 0 && bottom_y < canvas_height as i32 {
                let idx = ((bottom_y as u32 * canvas_width + px as u32) * 4) as usize;
                buffer[idx + 0] = border_color.0;
                buffer[idx + 1] = border_color.1;
                buffer[idx + 2] = border_color.2;
                buffer[idx + 3] = 0xff;
            }
        }
        // Left border
        for dy in 0..height {
            let py = y + dy as i32;
            if py >= 0 && py < canvas_height as i32 && x >= 0 && x < canvas_width as i32 {
                let idx = ((py as u32 * canvas_width + x as u32) * 4) as usize;
                buffer[idx + 0] = border_color.0;
                buffer[idx + 1] = border_color.1;
                buffer[idx + 2] = border_color.2;
                buffer[idx + 3] = 0xff;
            }
        }
        // Right border
        let right_x = x + width as i32 - 1;
        for dy in 0..height {
            let py = y + dy as i32;
            if py >= 0 && py < canvas_height as i32 && right_x >= 0 && right_x < canvas_width as i32 {
                let idx = ((py as u32 * canvas_width + right_x as u32) * 4) as usize;
                buffer[idx + 0] = border_color.0;
                buffer[idx + 1] = border_color.1;
                buffer[idx + 2] = border_color.2;
                buffer[idx + 3] = 0xff;
            }
        }
        
        // Note: Button text rendering is not implemented yet
        // The button will just show as a colored rectangle
    }

}

impl Application for CoderApp {
    fn setup(&mut self, state: &mut EngineState) -> Result<(), String> {
        // Setup all text apps
        self.code_app.setup(state)?;
        self.terminal_app.setup(state)?;
        self.console_app.setup(state)?;
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        // Process onscreen keyboard input for console (on terminal tab)
        if self.active_tab == Tab::Terminal {
            // Process any pending keyboard characters for console
            while let Some(ch) = state.keyboard.onscreen.pop_pending_char() {
                // Check if it's Enter - execute and clear console
                if ch == '\n' || ch == '\r' {
                    let command = self.console_app.text_rasterizer.text.clone();
                    if !command.trim().is_empty() {
                        self.execute_console_command(&command);
                        self.console_app.text_rasterizer.text.clear();
                        self.console_app.cursor_position = 0;
                    }
                } else {
                    // Pass other characters to console
                    self.console_app.on_key_char(state, ch);
                }
            }
        }
        
        // Get dimensions before mutable borrow
        let shape = state.frame.array.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        let mouse_x = state.mouse.x;
        let mouse_y = state.mouse.y;
        
        // Get keyboard top edge coordinates (normalized 0-1)
        let (_, keyboard_top_y, _, _) = state.keyboard.onscreen.top_edge_coordinates();
        let keyboard_top_px = keyboard_top_y * height;
        
        // Calculate button/tab dimensions
        let (_, button_height) = Self::get_button_size();
        let padding = 10;
        
        // Console is only shown on terminal tab
        let show_console = self.active_tab == Tab::Terminal;
        let console_height = button_height;
        
        // Calculate positions from bottom up:
        // 1. Keyboard at keyboard_top_px
        // 2. Tabs/button above keyboard (or above console if shown)
        // 3. Console above tabs/button (only if shown on terminal tab)
        
        let tabs_bottom_y = keyboard_top_px - padding as f32;
        let tabs_top_y = tabs_bottom_y - button_height as f32;
        
        let console_bottom_y = if show_console {
            tabs_top_y - padding as f32
        } else {
            tabs_bottom_y // Not shown, but calculate for consistency
        };
        let console_top_y = console_bottom_y - console_height as f32;
        
        // Delegate to active text app (but not console - it gets special handling)
        match self.active_tab {
            Tab::Code => {
                if self.code_view_mode == CodeViewMode::Editor {
                    self.code_app.tick(state)
                }
                // File explorer is drawn separately in the buffer drawing code below
            }
            Tab::Terminal => self.terminal_app.tick(state),
            Tab::Viewport => {
                // Render Python app if available, otherwise show black screen
                if let Some(ref app_instance) = self.viewport_app {
                    // Clear to black first
                    let buffer = state.frame_buffer_mut();
                    for i in (0..buffer.len()).step_by(4) {
                        buffer[i + 0] = 0; // R
                        buffer[i + 1] = 0; // G
                        buffer[i + 2] = 0; // B
                        buffer[i + 3] = 0xff; // A
                    }
                    
                    // Setup Python app if not done yet
                    if !self.viewport_app_setup_done {
                        let setup_result = self.interpreter.enter(|vm| {
                            // Register Application class and _FrameWrapper in builtins
                            let app_class_code = crate::python::engine::pyapp::APPLICATION_CLASS_CODE;
                            let scope = vm.new_scope_with_builtins();
                            if let Err(e) = vm.run_code_string(scope, app_class_code, "<viewport_setup>".to_string()) {
                                eprintln!("Failed to register Application class: {:?}", e);
                                return Err(());
                            }
                            
                            // Create Python frame object from engine state
                            let frame_dict = crate::python::engine::py_bindings::create_py_frame_state(vm, &mut state.frame)
                                .map_err(|e| { eprintln!("Failed to create frame object: {:?}", e); () })?;
                            
                            // Wrap it in _FrameWrapper
                            if let Ok(wrapper_class) = vm.builtins.get_attr("_FrameWrapper", vm) {
                                if let Ok(frame_obj) = wrapper_class.call((frame_dict.clone(),), vm) {
                                    app_instance.set_attr("frame", frame_obj, vm)
                                        .map_err(|e| { eprintln!("Failed to set frame attribute: {:?}", e); () })?;
                                } else {
                                    app_instance.set_attr("frame", frame_dict, vm)
                                        .map_err(|e| { eprintln!("Failed to set frame attribute: {:?}", e); () })?;
                                }
                            } else {
                                app_instance.set_attr("frame", frame_dict, vm)
                                    .map_err(|e| { eprintln!("Failed to set frame attribute: {:?}", e); () })?;
                            }
                            
                            // Create mouse object
                            let mouse_dict = vm.ctx.new_dict();
                            let _ = mouse_dict.set_item("x", vm.ctx.new_float(state.mouse.x as f64).into(), vm);
                            let _ = mouse_dict.set_item("y", vm.ctx.new_float(state.mouse.y as f64).into(), vm);
                            let _ = mouse_dict.set_item("is_left_clicking", vm.ctx.new_bool(state.mouse.is_left_clicking).into(), vm);
                            app_instance.set_attr("mouse", mouse_dict, vm)
                                .map_err(|e| { eprintln!("Failed to set mouse attribute: {:?}", e); () })?;
                            
                            // Call setup
                            if let Err(e) = vm.call_method(app_instance, "setup", ()) {
                                let class_name = e.class().name().to_string();
                                let msg = vm.call_method(e.as_object(), "__str__", ())
                                    .ok()
                                    .and_then(|result| result.str(vm).ok().map(|s| s.to_string()))
                                    .unwrap_or_default();
                                
                                if msg.is_empty() {
                                    eprintln!("Python setup error: {}", class_name);
                                } else {
                                    eprintln!("Python setup error: {}: {}", class_name, msg);
                                }
                                return Err(());
                            }
                            
                            Ok(())
                        });
                        
                        if setup_result.is_ok() {
                            self.viewport_app_setup_done = true;
                        }
                    }
                    
                    // Tick the Python app
                    if self.viewport_app_setup_done {
                        // Set the frame buffer context for the rasterizer
                        let shape = state.frame.array.shape();
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
                            }
                        });
                        
                        // Clear the frame buffer context after tick
                        crate::python::rasterizer::clear_frame_buffer_context();
                    }
                } else {
                    // No Python app - show black screen
                    let buffer = state.frame_buffer_mut();
                    for i in (0..buffer.len()).step_by(4) {
                        buffer[i + 0] = 0; // R
                        buffer[i + 1] = 0; // G
                        buffer[i + 2] = 0; // B
                        buffer[i + 3] = 0xff; // A
                    }
                }
            }
        }
        
        // Tick console separately with its own viewport
        // Console needs to know its available space for rendering
        self.console_app.text_rasterizer.tick(width, console_height as f32);
        
        // Update tab label rasterizers
        self.code_tab_label.tick(width, height);
        self.terminal_tab_label.tick(width, height);
        self.viewport_tab_label.tick(width, height);
        
        // Update clear button label
        self.clear_button_label.tick(width, height);
        
        // Get buffer again for drawing console, file explorer, tabs and buttons on top
        let buffer = state.frame_buffer_mut();
        
        // Draw file explorer if in file explorer mode on code tab
        if self.active_tab == Tab::Code && self.code_view_mode == CodeViewMode::FileExplorer {
            // Calculate the viewport height (everything above the tabs)
            let viewport_height = tabs_top_y;
            self.draw_file_explorer(buffer, width as u32, height as u32, viewport_height);
        }
        
        // Draw console above tabs (only when on terminal tab)
        if show_console {
            // Draw console area background (above keyboard)
            let console_bg_color = (20, 20, 20);
            for y in (console_top_y as i32)..(console_bottom_y as i32) {
                if y >= 0 && y < height as i32 {
                    for x in 0..(width as i32) {
                        let idx = ((y as u32 * width as u32 + x as u32) * 4) as usize;
                        buffer[idx + 0] = console_bg_color.0;
                        buffer[idx + 1] = console_bg_color.1;
                        buffer[idx + 2] = console_bg_color.2;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
            
            // Draw console text
            let text_color = (0, 255, 0); // Green terminal-style text
            for character in &self.console_app.text_rasterizer.characters {
                let px = character.x as i32;
                let py = (console_top_y + character.y - self.console_app.scroll_y) as i32;
                
                for y in 0..character.metrics.height {
                    for x in 0..character.metrics.width {
                        let val = character.bitmap[y * character.metrics.width + x];
                        
                        let sx = px + x as i32;
                        let sy = py + y as i32;
                        
                        if sx >= 0 && sx < width as i32 && sy >= console_top_y as i32 && sy < console_bottom_y as i32 {
                            let idx = ((sy as u32 * width as u32 + sx as u32) * 4) as usize;
                            buffer[idx + 0] = ((text_color.0 as u16 * val as u16) / 255) as u8;
                            buffer[idx + 1] = ((text_color.1 as u16 * val as u16) / 255) as u8;
                            buffer[idx + 2] = ((text_color.2 as u16 * val as u16) / 255) as u8;
                            buffer[idx + 3] = val;
                        }
                    }
                }
            }
            
            // Draw console cursor
            if self.console_app.show_cursor {
                // Find cursor position
                let line_info_with_idx = self.console_app.text_rasterizer.lines.iter()
                    .enumerate()
                    .find(|(_, line)| {
                        line.start_index <= self.console_app.cursor_position && self.console_app.cursor_position <= line.end_index
                    });
                
                let (cursor_x, baseline_y) = if let Some((line_idx, line)) = line_info_with_idx {
                    let chars_in_line: Vec<_> = self.console_app.text_rasterizer.characters.iter()
                        .filter(|c| c.line_index == line_idx)
                        .collect();
                    
                    if chars_in_line.is_empty() || self.console_app.cursor_position == line.start_index {
                        (0.0, line.baseline_y)
                    } else if let Some(last_char) = chars_in_line.last() {
                        if self.console_app.cursor_position > last_char.char_index {
                            (last_char.x + last_char.metrics.advance_width, line.baseline_y)
                        } else {
                            (0.0, line.baseline_y)
                        }
                    } else {
                        (0.0, line.baseline_y)
                    }
                } else if let Some(first_line) = self.console_app.text_rasterizer.lines.first() {
                    (0.0, first_line.baseline_y)
                } else {
                    (0.0, self.console_app.text_rasterizer.ascent)
                };
                
                let cursor_top = (console_top_y + baseline_y - self.console_app.text_rasterizer.ascent - self.console_app.scroll_y).round() as i32;
                let cursor_bottom = (console_top_y + baseline_y + self.console_app.text_rasterizer.descent - self.console_app.scroll_y).round() as i32;
                let cx = cursor_x.round() as i32;
                
                for y in cursor_top..cursor_bottom {
                    if y >= console_top_y as i32 && y < console_bottom_y as i32 && cx >= 0 && cx < width as i32 {
                        let idx = ((y as u32 * width as u32 + cx as u32) * 4) as usize;
                        buffer[idx + 0] = text_color.0;
                        buffer[idx + 1] = text_color.1;
                        buffer[idx + 2] = text_color.2;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
            
            // Draw console border
            let border_color = (100, 100, 100);
            // Top border
            for x in 0..(width as i32) {
                let y = console_top_y as i32;
                if y >= 0 && y < height as i32 {
                    let idx = ((y as u32 * width as u32 + x as u32) * 4) as usize;
                    buffer[idx + 0] = border_color.0;
                    buffer[idx + 1] = border_color.1;
                    buffer[idx + 2] = border_color.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }
        
        // Draw tabs at tabs_top_y position (narrower on iOS)
        let tab_top_y = tabs_top_y as i32;
        let tab_width = Self::get_tab_width();
        let tab_height = button_height;
        
        // Draw code.py tab on the left
        self.draw_tab(buffer, width as u32, height as u32, padding, tab_top_y, tab_width, tab_height, &self.code_tab_label, self.active_tab == Tab::Code);
        
        // Draw terminal tab next to it
        self.draw_tab(buffer, width as u32, height as u32, padding + tab_width as i32, tab_top_y, tab_width, tab_height, &self.terminal_tab_label, self.active_tab == Tab::Terminal);
        
        // Draw viewport tab next to terminal
        self.draw_tab(buffer, width as u32, height as u32, padding + (tab_width * 2) as i32, tab_top_y, tab_width, tab_height, &self.viewport_tab_label, self.active_tab == Tab::Viewport);
        
        // Check if viewport app is running
        let is_app_running = self.viewport_app.is_some();
        
        // Position buttons on the right side
        // If app is running, show stop button to the left of run button
        if is_app_running {
            // Stop button to the left, run button to the right
            self.run_button.x = (width as i32) - (self.run_button.width as i32) - padding;
            self.run_button.y = tab_top_y;
            
            self.stop_button.x = self.run_button.x - (self.stop_button.width as i32) - (padding / 2);
            self.stop_button.y = tab_top_y;
            
            // Draw stop button
            let is_stop_hovered = self.stop_button.contains_point(mouse_x, mouse_y);
            self.stop_button.draw(buffer, width as u32, height as u32, is_stop_hovered);
        } else {
            // Just run button
            self.run_button.x = (width as i32) - (self.run_button.width as i32) - padding;
            self.run_button.y = tab_top_y;
        }
        
        // Determine button behavior and color based on console state
        let console_has_text = !self.console_app.text_rasterizer.text.trim().is_empty();
        let should_execute_console = show_console && console_has_text;
        
        // Check if mouse is hovering over run button
        let is_run_hovered = self.run_button.contains_point(mouse_x, mouse_y);
        
        // Draw run button with appropriate color
        if should_execute_console {
            // Gold color for console command
            self.draw_button_with_color(buffer, width as u32, height as u32, is_run_hovered, (218, 165, 32));
        } else {
            // Green color for running code
            self.run_button.draw(buffer, width as u32, height as u32, is_run_hovered);
        }
        
        // Position and draw clear "×" (only when console is shown and has text)
        if show_console && console_has_text {
            // Position clear button on right side of console, vertically centered in console area
            self.clear_button.x = self.run_button.x + (self.run_button.width as i32 - self.clear_button.width as i32);
            let console_center_y = (console_top_y + console_bottom_y) / 2.0;
            self.clear_button.y = (console_center_y - (self.clear_button.height as f32 / 2.0)) as i32;
            
            // Check if mouse is hovering over clear button area
            let is_clear_hovered = self.clear_button.contains_point(mouse_x, mouse_y);
            
            // Draw "×" label (no background, just the character)
            // Use gray when not hovered, lighter gray when hovered
            let text_color = if is_clear_hovered {
                (180, 180, 180) // Lighter gray on hover
            } else {
                (120, 120, 120) // Gray
            };
            
            for character in &self.clear_button_label.characters {
                // Center the text in the button area
                let text_width = self.clear_button_label.characters.iter()
                    .map(|c| c.metrics.advance_width)
                    .sum::<f32>();
                let text_offset_x = (self.clear_button.width as f32 - text_width) / 2.0;
                let text_offset_y = (self.clear_button.height as f32 - self.clear_button_label.font_size) / 2.0;
                
                let char_x = self.clear_button.x as f32 + character.x + text_offset_x;
                let char_y = self.clear_button.y as f32 + character.y + text_offset_y;
                
                for (bitmap_y, row) in character.bitmap.chunks(character.width as usize).enumerate() {
                    for (bitmap_x, &alpha) in row.iter().enumerate() {
                        if alpha == 0 {
                            continue;
                        }
                        
                        let px = (char_x + bitmap_x as f32) as i32;
                        let py = (char_y + bitmap_y as f32) as i32;
                        
                        if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                            let idx = ((py as u32 * width as u32 + px as u32) * 4) as usize;
                            
                            // Blend text color with alpha
                            let alpha_f = alpha as f32 / 255.0;
                            buffer[idx + 0] = ((text_color.0 as f32 * alpha_f) + (buffer[idx + 0] as f32 * (1.0 - alpha_f))) as u8;
                            buffer[idx + 1] = ((text_color.1 as f32 * alpha_f) + (buffer[idx + 1] as f32 * (1.0 - alpha_f))) as u8;
                            buffer[idx + 2] = ((text_color.2 as f32 * alpha_f) + (buffer[idx + 2] as f32 * (1.0 - alpha_f))) as u8;
                        }
                    }
                }
            }
        }
    }

    fn on_scroll(&mut self, state: &mut EngineState, dx: f32, dy: f32) {
        match self.active_tab {
            Tab::Code => {
                if self.code_view_mode == CodeViewMode::FileExplorer {
                    // Scroll the file list
                    self.file_list_scroll_y += dy;
                    // Clamp to valid range
                    let max_scroll = (self.python_files.len() as f32 * 60.0).max(0.0);
                    self.file_list_scroll_y = self.file_list_scroll_y.max(0.0).min(max_scroll);
                } else {
                    self.code_app.on_scroll(state, dx, dy)
                }
            }
            Tab::Terminal => self.terminal_app.on_scroll(state, dx, dy),
            Tab::Viewport => {
                // No scrolling in viewport
            }
        }
    }

    fn on_key_char(&mut self, state: &mut EngineState, ch: char) {
        match self.active_tab {
            Tab::Code => {
                // On code tab, input goes to code editor (only if not in file explorer)
                if self.code_view_mode == CodeViewMode::Editor {
                    self.code_app.on_key_char(state, ch);
                }
            }
            Tab::Terminal => {
                // On terminal tab, check if Enter key - execute console command
                if ch == '\n' || ch == '\r' {
                    // Execute console command
                    let command = self.console_app.text_rasterizer.text.clone();
                    self.execute_console_command(&command);
                    // Clear the console input after execution
                    self.console_app.text_rasterizer.text.clear();
                    self.console_app.cursor_position = 0;
                    return;
                }
                
                // Pass all other characters to console (it's our interactive terminal)
                self.console_app.on_key_char(state, ch);
            }
            Tab::Viewport => {
                // No keyboard input in viewport
            }
        }
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        match self.active_tab {
            Tab::Code => {
                if self.code_view_mode == CodeViewMode::Editor {
                    self.code_app.on_mouse_move(state)
                }
            }
            Tab::Terminal => self.terminal_app.on_mouse_move(state),
            Tab::Viewport => {
                // Forward to Python app if available
                if let Some(ref app_instance) = self.viewport_app {
                    let mouse_x = state.mouse.x;
                    let mouse_y = state.mouse.y;
                    self.interpreter.enter(|vm| {
                        let _ = vm.call_method(app_instance, "on_mouse_move", (mouse_x, mouse_y));
                    });
                }
            }
        }
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        let mouse_x = state.mouse.x;
        let mouse_y = state.mouse.y;
        
        println!("Mouse down at ({}, {})", mouse_x, mouse_y);
        
        // Tab dimensions (must match tick())
        let (_, tab_height) = Self::get_button_size();
        let tab_width = Self::get_tab_width();
        let padding = 10;
        
        // Calculate tab position (same as in tick)
        let shape = state.frame.array.shape();
        let height = shape[0] as f32;
        let (_, keyboard_top_y, _, _) = state.keyboard.onscreen.top_edge_coordinates();
        let keyboard_top_px = keyboard_top_y * height;
        
        // Tabs are always at keyboard edge now (console is above them)
        let tabs_bottom_y = keyboard_top_px - padding as f32;
        let tabs_top_y = tabs_bottom_y - tab_height as f32;
        let tab_top_y = tabs_top_y as i32;
        
        println!("Button position - x: {}, y: {}, width: {}, height: {}", 
                 self.run_button.x, self.run_button.y, self.run_button.width, self.run_button.height);
        println!("Tab position - y: {}, height: {}", tab_top_y, tab_height);
        
        // Check if click is on code.py tab
        if self.tab_contains_point(mouse_x, mouse_y, padding, tab_top_y, tab_width, tab_height) {
            println!("Code tab clicked");
            if self.active_tab == Tab::Code {
                // Already on code tab - toggle file explorer
                match self.code_view_mode {
                    CodeViewMode::Editor => {
                        // Save current file before switching to explorer
                        self.save_current_file();
                        self.code_view_mode = CodeViewMode::FileExplorer;
                        println!("Switched to file explorer");
                    }
                    CodeViewMode::FileExplorer => {
                        self.code_view_mode = CodeViewMode::Editor;
                        println!("Switched to editor");
                    }
                }
            } else {
                // Switching from another tab - go to editor mode
                self.active_tab = Tab::Code;
                self.code_view_mode = CodeViewMode::Editor;
            }
            return;
        }
        
        // Check if click is on terminal tab
        if self.tab_contains_point(mouse_x, mouse_y, padding + tab_width as i32, tab_top_y, tab_width, tab_height) {
            println!("Terminal tab clicked");
            self.active_tab = Tab::Terminal;
            return;
        }
        
        // Check if click is on viewport tab
        if self.tab_contains_point(mouse_x, mouse_y, padding + (tab_width * 2) as i32, tab_top_y, tab_width, tab_height) {
            println!("Viewport tab clicked");
            self.active_tab = Tab::Viewport;
            return;
        }
        
        // Check if click is on clear button (only visible when console has text)
        let console_has_text = !self.console_app.text_rasterizer.text.trim().is_empty();
        if self.active_tab == Tab::Terminal && console_has_text {
            if self.clear_button.contains_point(mouse_x, mouse_y) {
                println!("Clear button clicked");
                // Clear the console input
                self.console_app.text_rasterizer.text.clear();
                self.console_app.cursor_position = 0;
                return;
            }
        }
        
        // Check if click is on the stop button (only visible when app is running)
        if self.viewport_app.is_some() && self.stop_button.contains_point(mouse_x, mouse_y) {
            println!("Stop button clicked - stopping viewport app");
            // Stop the viewport app
            self.viewport_app = None;
            self.viewport_app_setup_done = false;
            
            // Clear the terminal to show stop message
            self.terminal_app.text_rasterizer.text.push_str("\n[xos] Viewport app stopped\n");
            return;
        }
        
        // Check if click is on the run button
        if self.run_button.contains_point(mouse_x, mouse_y) {
            println!("Run button clicked at ({}, {})", mouse_x, mouse_y);
            
            // Determine if we should execute console command or run code
            let show_console = self.active_tab == Tab::Terminal;
            let console_has_text = !self.console_app.text_rasterizer.text.trim().is_empty();
            
            if show_console && console_has_text {
                // Execute console command and clear it
                println!("Executing console command");
                let command = self.console_app.text_rasterizer.text.clone();
                self.execute_console_command(&command);
                // Clear the console input after execution
                self.console_app.text_rasterizer.text.clear();
                self.console_app.cursor_position = 0;
            } else {
                // Execute the Python code from code tab
                let code = self.code_app.text_rasterizer.text.clone();
                println!("Code to execute: {}", code);
                if !code.trim().is_empty() {
                    self.execute_python_code(&code);
                } else {
                    println!("Code was empty!");
                }
            }
            return;
        }
        
        println!("Click not on any button, checking file explorer or delegating to text app");
        
        // Check if we're in file explorer mode and clicked on a file
        if self.active_tab == Tab::Code && self.code_view_mode == CodeViewMode::FileExplorer {
            // Calculate file list item positions (same as in draw_file_explorer)
            let item_height = 60.0;
            
            // Check if click is within the file list area
            if mouse_y < tabs_top_y {
                let clicked_index = ((mouse_y + self.file_list_scroll_y) / item_height) as usize;
                if clicked_index < self.python_files.len() {
                    println!("Clicked on file: {}", self.python_files[clicked_index].name);
                    self.load_file(clicked_index);
                    return;
                }
            }
        }
        
        // Otherwise delegate to active text app or handle viewport
        match self.active_tab {
            Tab::Code => {
                if self.code_view_mode == CodeViewMode::Editor {
                    self.code_app.on_mouse_down(state)
                }
            }
            Tab::Terminal => self.terminal_app.on_mouse_down(state),
            Tab::Viewport => {
                // Handle double-tap to show/hide keyboard in viewport
                use std::time::{Duration, Instant};
                const DOUBLE_TAP_TIME_MS: u64 = 300;
                const DOUBLE_TAP_DISTANCE: f32 = 50.0;
                
                let now = Instant::now();
                let is_double_tap = if let Some(last_time) = self.viewport_last_tap_time {
                    let time_since_last = now.duration_since(last_time);
                    let distance = ((mouse_x - self.viewport_last_tap_x).powi(2) + (mouse_y - self.viewport_last_tap_y).powi(2)).sqrt();
                    
                    time_since_last < Duration::from_millis(DOUBLE_TAP_TIME_MS) && distance < DOUBLE_TAP_DISTANCE
                } else {
                    false
                };
                
                if is_double_tap {
                    // Toggle keyboard
                    state.keyboard.onscreen.toggle_minimize();
                    // Reset tap tracking
                    self.viewport_last_tap_time = None;
                } else {
                    // Update tap tracking
                    self.viewport_last_tap_time = Some(now);
                    self.viewport_last_tap_x = mouse_x;
                    self.viewport_last_tap_y = mouse_y;
                    
                    // Forward to Python app if available
                    if let Some(ref app_instance) = self.viewport_app {
                        self.interpreter.enter(|vm| {
                            let _ = vm.call_method(app_instance, "on_mouse_down", (mouse_x, mouse_y));
                        });
                    }
                }
            }
        }
    }

    fn on_mouse_up(&mut self, state: &mut EngineState) {
        match self.active_tab {
            Tab::Code => {
                if self.code_view_mode == CodeViewMode::Editor {
                    self.code_app.on_mouse_up(state)
                }
            }
            Tab::Terminal => self.terminal_app.on_mouse_up(state),
            Tab::Viewport => {
                // Forward to Python app if available
                if let Some(ref app_instance) = self.viewport_app {
                    let mouse_x = state.mouse.x;
                    let mouse_y = state.mouse.y;
                    self.interpreter.enter(|vm| {
                        let _ = vm.call_method(app_instance, "on_mouse_up", (mouse_x, mouse_y));
                    });
                }
            }
        }
    }
}

