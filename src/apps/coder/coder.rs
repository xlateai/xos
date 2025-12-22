use crate::engine::{Application, EngineState};
use crate::apps::text::text::TextApp;
use crate::apps::coder::button::Button;
use crate::text::text_rasterization::TextRasterizer;
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
    pub interpreter: Interpreter,
    pub run_button: Button,
    pub code_tab_label: TextRasterizer,
    pub terminal_tab_label: TextRasterizer,
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
    
    pub fn new() -> Self {
        // Create the code editor text app with default code
        let mut code_app = TextApp::new();
        code_app.text_rasterizer.text = r#"print("Hello World! Double tap screen to show keyboard")"#.to_string();
        
        // Create the terminal text app with prompt
        let mut terminal_app = TextApp::new();
        terminal_app.text_rasterizer.text = "Run code.py".to_string();

        // Initialize RustPython interpreter with xos module
        let interpreter = Interpreter::with_init(Default::default(), |vm| {
            // Register the xos native module
            vm.add_native_module("xos".to_owned(), Box::new(crate::python::xos_module::make_module));
        });

        // Create run button (position will be updated in tick)
        let (button_width, button_height) = Self::get_button_size();
        let run_button = Button::new(0, 0, button_width, button_height, "Run".to_string());
        
        // Load font for tab labels
        let font_data = include_bytes!("../../../assets/JetBrainsMono-Regular.ttf");
        let font = fontdue::Font::from_bytes(
            font_data as &[u8],
            fontdue::FontSettings::default(),
        ).expect("Failed to load font");
        
        // Create text rasterizers for tab labels
        let mut code_tab_label = TextRasterizer::new(font.clone(), 20.0);
        code_tab_label.set_text("code.py".to_string());
        
        let mut terminal_tab_label = TextRasterizer::new(font, 20.0);
        terminal_tab_label.set_text("terminal".to_string());

        Self {
            code_app,
            terminal_app,
            active_tab: Tab::Code,
            interpreter,
            run_button,
            code_tab_label,
            terminal_tab_label,
        }
    }

    fn execute_python_code(&mut self, code: &str) {
        // Clear terminal - just show the raw output
        self.terminal_app.text_rasterizer.text.clear();
        
        let result = self.interpreter.enter(|vm| {
            let scope = vm.new_scope_with_builtins();
            
            // Set __name__ to "__main__" so if __name__ == "__main__" works
            let _ = scope.globals.set_item("__name__", vm.ctx.new_str("__main__").into(), vm);
            
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
            if let Ok(Some(_app_instance_obj)) = vm.get_attribute_opt(vm.builtins.as_object().to_owned(), "__xos_app_instance__") {
                extra_output.push_str("[xos] Application instance registered in coder!\n");
                extra_output.push_str("[xos] Note: The coder app cannot launch the xos engine window.\n");
                extra_output.push_str("[xos] To run this application with a window, save it to a file and run:\n");
                extra_output.push_str("[xos]   xos python <filename>.py\n");
            }
            
            Ok((captured_output, extra_output))
        });

        // Display output in terminal
        match result {
            Ok((output, extra)) => {
                self.terminal_app.text_rasterizer.text = output + &extra;
                if self.terminal_app.text_rasterizer.text.trim().is_empty() {
                    self.terminal_app.text_rasterizer.text = "(no output)".to_string();
                }
            }
            Err((error, output)) => {
                self.terminal_app.text_rasterizer.text = output + &error + "\n";
            }
        }
        
        // Switch to terminal tab to show output
        self.active_tab = Tab::Terminal;
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
        
        // Update tab label rasterizers
        self.code_tab_label.tick(width, height);
        self.terminal_tab_label.tick(width, height);
        
        // Get buffer again for drawing tabs and button on top
        let buffer = state.frame_buffer_mut();
        
        // Draw tabs aligned with keyboard edge - same size as button
        let (tab_width, tab_height) = Self::get_button_size();
        let padding = 10;
        
        // Position tabs just above the keyboard (same logic as button)
        let keyboard_top_px = keyboard_top_y * height;
        let tab_bottom_y = keyboard_top_px - padding as f32;
        let tab_top_y = (tab_bottom_y - tab_height as f32) as i32;
        
        // Draw code.py tab on the left
        self.draw_tab(buffer, width as u32, height as u32, padding, tab_top_y, tab_width, tab_height, &self.code_tab_label, self.active_tab == Tab::Code);
        
        // Draw terminal tab next to it
        self.draw_tab(buffer, width as u32, height as u32, padding + tab_width as i32, tab_top_y, tab_width, tab_height, &self.terminal_tab_label, self.active_tab == Tab::Terminal);
        
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
        
        println!("Mouse down at ({}, {})", mouse_x, mouse_y);
        
        // Tab dimensions (must match tick())
        let (tab_width, tab_height) = Self::get_button_size();
        let padding = 10;
        
        // Calculate tab position (same as in tick)
        let shape = state.frame.array.shape();
        let height = shape[0] as f32;
        let (_, keyboard_top_y, _, _) = state.keyboard.onscreen.top_edge_coordinates();
        let keyboard_top_px = keyboard_top_y * height;
        let tab_bottom_y = keyboard_top_px - padding as f32;
        let tab_top_y = (tab_bottom_y - tab_height as f32) as i32;
        
        println!("Button position - x: {}, y: {}, width: {}, height: {}", 
                 self.run_button.x, self.run_button.y, self.run_button.width, self.run_button.height);
        println!("Tab position - y: {}, height: {}", tab_top_y, tab_height);
        
        // Check if click is on code.py tab
        if self.tab_contains_point(mouse_x, mouse_y, padding, tab_top_y, tab_width, tab_height) {
            println!("Code tab clicked");
            self.active_tab = Tab::Code;
            return;
        }
        
        // Check if click is on terminal tab
        if self.tab_contains_point(mouse_x, mouse_y, padding + tab_width as i32, tab_top_y, tab_width, tab_height) {
            println!("Terminal tab clicked");
            self.active_tab = Tab::Terminal;
            return;
        }
        
        // Check if click is on the run button
        if self.run_button.contains_point(mouse_x, mouse_y) {
            println!("Run button clicked at ({}, {})", mouse_x, mouse_y);
            // Execute the Python code from code tab
            let code = self.code_app.text_rasterizer.text.clone();
            println!("Code to execute: {}", code);
            if !code.trim().is_empty() {
                self.execute_python_code(&code);
            } else {
                println!("Code was empty!");
            }
            return;
        }
        
        println!("Click not on any button, delegating to text app");
        
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

