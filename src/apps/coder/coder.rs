use crate::engine::{Application, EngineState};
use crate::apps::text::text::TextApp;
use crate::apps::coder::button::Button;

#[cfg(feature = "python")]
use rustpython_vm::{Interpreter, AsObject};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Tab {
    Code,
    Terminal,
}

pub struct CoderApp {
    pub code_app: TextApp,
    pub terminal_app: TextApp,
    active_tab: Tab,
    #[cfg(feature = "python")]
    pub interpreter: Interpreter,
    pub run_button: Button,
}

impl CoderApp {
    pub fn new() -> Self {
        // Create the code editor text app
        let code_app = TextApp::new();
        
        // Create the terminal text app (read-only for now)
        let terminal_app = TextApp::new();

        // Initialize RustPython interpreter with xos module
        #[cfg(feature = "python")]
        let interpreter = Interpreter::with_init(Default::default(), |vm| {
            // Register the xos native module
            vm.add_native_module("xos".to_owned(), Box::new(crate::python::xos_module::make_module));
        });

        // Create run button (position will be updated in tick)
        let run_button = Button::new(0, 0, 160, 60, "Run".to_string());

        Self {
            code_app,
            terminal_app,
            active_tab: Tab::Code,
            #[cfg(feature = "python")]
            interpreter,
            run_button,
        }
    }

    #[cfg(feature = "python")]
    fn execute_python_code(&mut self, code: &str) {
        use std::io::Write;
        
        // Clear terminal and add header
        self.terminal_app.text_rasterizer.text.clear();
        self.terminal_app.text_rasterizer.text.push_str("=== Executing Python Code ===\n");
        
        // Capture stdout using a custom writer
        let mut output_buffer = Vec::new();
        
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
                
                let error_text = if !error_msg.is_empty() {
                    format!("Python Error: {}: {}", class_name, error_msg)
                } else {
                    format!("Python Error: {}", class_name)
                };
                
                writeln!(&mut output_buffer, "{}", error_text).ok();
                return Err(error_text);
            }
            
            // Check if an xos.Application was registered
            if let Ok(Some(_app_instance_obj)) = vm.get_attribute_opt(vm.builtins.as_object().to_owned(), "__xos_app_instance__") {
                writeln!(&mut output_buffer, "\n[xos] Application instance registered in coder!").ok();
                writeln!(&mut output_buffer, "[xos] Note: The coder app cannot launch the xos engine window.").ok();
                writeln!(&mut output_buffer, "[xos] To run this application with a window, save it to a file and run:").ok();
                writeln!(&mut output_buffer, "[xos]   xos python <filename>.py").ok();
            }
            
            Ok(())
        });

        // Add output to terminal
        if let Ok(output_str) = String::from_utf8(output_buffer) {
            self.terminal_app.text_rasterizer.text.push_str(&output_str);
        }
        
        match result {
            Ok(_) => {
                self.terminal_app.text_rasterizer.text.push_str("\n--- Execution Complete ---\n");
            }
            Err(_) => {
                self.terminal_app.text_rasterizer.text.push_str("\n--- Execution Failed ---\n");
            }
        }
        
        // Switch to terminal tab to show output
        self.active_tab = Tab::Terminal;
    }

    #[cfg(not(feature = "python"))]
    fn execute_python_code(&mut self, _code: &str) {
        println!("\n=== Python execution not available (python feature disabled) ===\n");
    }

    fn draw_tab(&self, buffer: &mut [u8], canvas_width: u32, canvas_height: u32, x: i32, y: i32, width: u32, height: u32, label: &str, is_active: bool) {
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
        
        // TODO: Draw label text using font rendering
        let _ = label; // Suppress unused warning for now
    }

    fn tab_contains_point(&self, x: f32, y: f32, tab_x: i32, tab_y: i32, tab_width: u32, tab_height: u32) -> bool {
        x >= tab_x as f32 
            && x < (tab_x + tab_width as i32) as f32
            && y >= tab_y as f32
            && y < (tab_y + tab_height as i32) as f32
    }

}

impl Application for CoderApp {
    fn setup(&mut self, state: &mut EngineState) -> Result<(), String> {
        // Setup both text apps
        self.code_app.setup(state)?;
        self.terminal_app.setup(state)?;
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        // Get dimensions before mutable borrow
        let shape = state.frame.array.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        let mouse_x = state.mouse.x;
        let mouse_y = state.mouse.y;
        
        // Get keyboard top edge coordinates (normalized 0-1)
        let (_, keyboard_top_y, _, _) = state.keyboard.onscreen.top_edge_coordinates();
        
        // Delegate to active text app
        match self.active_tab {
            Tab::Code => self.code_app.tick(state),
            Tab::Terminal => self.terminal_app.tick(state),
        }
        
        // Get buffer again for drawing tabs and button on top
        let buffer = state.frame_buffer_mut();
        
        // Draw tabs aligned with keyboard edge
        let tab_height = 40;
        let tab_width = 120;
        let padding = 10;
        
        // Position tabs just above the keyboard (same logic as button)
        let keyboard_top_px = keyboard_top_y * height;
        let tab_bottom_y = keyboard_top_px - padding as f32;
        let tab_top_y = (tab_bottom_y - tab_height as f32) as i32;
        
        // Draw code.py tab on the left
        self.draw_tab(buffer, width as u32, height as u32, padding, tab_top_y, tab_width, tab_height, "code.py", self.active_tab == Tab::Code);
        
        // Draw terminal tab next to it
        self.draw_tab(buffer, width as u32, height as u32, padding + tab_width as i32, tab_top_y, tab_width, tab_height, "terminal", self.active_tab == Tab::Terminal);
        
        // Position button on the right side, same vertical alignment as tabs
        let button_height = self.run_button.height as f32;
        let button_top_y = (tab_bottom_y - button_height) as i32;
        
        self.run_button.x = (width as i32) - (self.run_button.width as i32) - padding;
        self.run_button.y = button_top_y as i32;
        
        // Check if mouse is hovering over button
        let is_hovered = self.run_button.contains_point(mouse_x, mouse_y);
        
        // Draw run button on top of everything
        self.run_button.draw(buffer, width as u32, height as u32, is_hovered);
    }

    fn on_scroll(&mut self, state: &mut EngineState, dx: f32, dy: f32) {
        match self.active_tab {
            Tab::Code => self.code_app.on_scroll(state, dx, dy),
            Tab::Terminal => self.terminal_app.on_scroll(state, dx, dy),
        }
    }

    fn on_key_char(&mut self, state: &mut EngineState, ch: char) {
        // Only allow editing in code tab
        if self.active_tab == Tab::Code {
            self.code_app.on_key_char(state, ch);
        }
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        match self.active_tab {
            Tab::Code => self.code_app.on_mouse_move(state),
            Tab::Terminal => self.terminal_app.on_mouse_move(state),
        }
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        let mouse_x = state.mouse.x;
        let mouse_y = state.mouse.y;
        
        // Tab dimensions (must match tick())
        let tab_height = 40;
        let tab_width = 120;
        let padding = 10;
        
        // Calculate tab position (same as in tick)
        let shape = state.frame.array.shape();
        let height = shape[0] as f32;
        let (_, keyboard_top_y, _, _) = state.keyboard.onscreen.top_edge_coordinates();
        let keyboard_top_px = keyboard_top_y * height;
        let tab_bottom_y = keyboard_top_px - padding as f32;
        let tab_top_y = (tab_bottom_y - tab_height as f32) as i32;
        
        // Check if click is on code.py tab
        if self.tab_contains_point(mouse_x, mouse_y, padding, tab_top_y, tab_width, tab_height) {
            self.active_tab = Tab::Code;
            return;
        }
        
        // Check if click is on terminal tab
        if self.tab_contains_point(mouse_x, mouse_y, padding + tab_width as i32, tab_top_y, tab_width, tab_height) {
            self.active_tab = Tab::Terminal;
            return;
        }
        
        // Check if click is on the run button
        if self.run_button.contains_point(mouse_x, mouse_y) {
            // Execute the Python code from code tab
            let code = self.code_app.text_rasterizer.text.clone();
            if !code.trim().is_empty() {
                self.execute_python_code(&code);
            }
            return;
        }
        
        // Otherwise delegate to active text app
        match self.active_tab {
            Tab::Code => self.code_app.on_mouse_down(state),
            Tab::Terminal => self.terminal_app.on_mouse_down(state),
        }
    }

    fn on_mouse_up(&mut self, state: &mut EngineState) {
        match self.active_tab {
            Tab::Code => self.code_app.on_mouse_up(state),
            Tab::Terminal => self.terminal_app.on_mouse_up(state),
        }
    }
}

