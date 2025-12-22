use crate::engine::{Application, EngineState};
use crate::apps::text::geometric::GeometricText;
use crate::apps::coder::button::Button;
use fontdue::{Font, FontSettings};
use std::collections::HashMap;

#[cfg(feature = "python")]
use rustpython_vm::{Interpreter, AsObject};

const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);
const TEXT_COLOR: (u8, u8, u8) = (255, 255, 255);
const CURSOR_COLOR: (u8, u8, u8) = (0, 255, 0);

pub struct CoderApp {
    pub text_engine: GeometricText,
    pub scroll_y: f32,
    pub smooth_cursor_x: f32,
    pub fade_map: HashMap<(char, u32, u32), f32>,
    pub cursor_position: usize, // Character index in the text
    #[cfg(feature = "python")]
    pub interpreter: Interpreter,
    pub run_button: Button,
}

impl CoderApp {
    pub fn new() -> Self {
        let font_bytes = include_bytes!("../../../assets/JetBrainsMono-Regular.ttf") as &[u8];
        let font = Font::from_bytes(font_bytes, FontSettings::default())
            .expect("Failed to load font");

        let text_engine = GeometricText::new(font, 24.0);

        // Initialize RustPython interpreter with xos module
        #[cfg(feature = "python")]
        let interpreter = Interpreter::with_init(Default::default(), |vm| {
            // Register the xos native module
            vm.add_native_module("xos".to_owned(), Box::new(crate::python::xos_module::make_module));
        });

        // Create run button (position will be updated in tick)
        let run_button = Button::new(0, 0, 80, 30, "Run".to_string());

        Self {
            text_engine,
            scroll_y: 0.0,
            smooth_cursor_x: 0.0,
            fade_map: HashMap::new(),
            cursor_position: 0,
            #[cfg(feature = "python")]
            interpreter,
            run_button,
        }
    }

    fn tick_cursor(&mut self, buffer: &mut [u8], width: u32, height: u32) {
        // Calculate cursor position based on cursor_position
        let text = &self.text_engine.text;
        let chars: Vec<char> = text.chars().collect();
        
        // Find the character at cursor_position
        let (target_x, baseline_y) = if self.cursor_position == 0 {
            (0.0, self.text_engine.ascent)
        } else if self.cursor_position >= chars.len() {
            // Cursor at end
            if let Some(last) = self.text_engine.characters.last() {
                if text.chars().last() == Some('\n') {
                    (0.0, self.text_engine.lines.last().map_or(self.text_engine.ascent, |line| line.baseline_y))
                } else {
                    (last.x + last.metrics.advance_width, self.text_engine.lines.last().map_or(self.text_engine.ascent, |line| line.baseline_y))
                }
            } else {
                (0.0, self.text_engine.ascent)
            }
        } else {
            // Find character at cursor position
            let mut char_idx = 0;
            let mut found = false;
            for (i, character) in self.text_engine.characters.iter().enumerate() {
                if character.char_index == self.cursor_position {
                    char_idx = i;
                    found = true;
                    break;
                }
            }
            
            if found {
                let character = &self.text_engine.characters[char_idx];
                (character.x, character.y + self.text_engine.ascent)
            } else {
                // Fallback: find line
                let mut line_baseline = self.text_engine.ascent;
                for line in &self.text_engine.lines {
                    if self.cursor_position >= line.start_index && self.cursor_position <= line.end_index {
                        line_baseline = line.baseline_y;
                        break;
                    }
                }
                (0.0, line_baseline)
            }
        };
        
        // Smooth the x-position (linear interpolation)
        self.smooth_cursor_x += (target_x - self.smooth_cursor_x) * 0.2;
        
        let cursor_top = (baseline_y - self.text_engine.ascent - self.scroll_y).round() as i32;
        let cursor_bottom = (baseline_y + self.text_engine.descent - self.scroll_y).round() as i32;
        let cx = self.smooth_cursor_x.round() as i32;
        
        for y in cursor_top..cursor_bottom {
            if y >= 0 && y < height as i32 && cx >= 0 && cx < width as i32 {
                let idx = ((y as u32 * width + cx as u32) * 4) as usize;
                buffer[idx + 0] = CURSOR_COLOR.0;
                buffer[idx + 1] = CURSOR_COLOR.1;
                buffer[idx + 2] = CURSOR_COLOR.2;
                buffer[idx + 3] = 0xff;
            }
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
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let shape = state.frame.array.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        
        // Get mouse coordinates before mutable borrow
        let mouse_x = state.mouse.x;
        let mouse_y = state.mouse.y;
        
        let buffer = state.frame_buffer_mut();
    
        self.text_engine.tick(width, height);
    
        // Clear screen
        for i in (0..buffer.len()).step_by(4) {
            buffer[i + 0] = BACKGROUND_COLOR.0;
            buffer[i + 1] = BACKGROUND_COLOR.1;
            buffer[i + 2] = BACKGROUND_COLOR.2;
            buffer[i + 3] = 0xff;
        }
    
        // Draw characters with fade and slide-in
        for character in &self.text_engine.characters {
            let fade_key = (character.ch, character.x.to_bits(), character.y.to_bits());
            let fade = self.fade_map.entry(fade_key).or_insert(1.0); // Start fully visible
            *fade = (*fade + 0.16).min(1.0);

            let alpha = (*fade * 255.0).round() as u8;

            let px = character.x as i32;
            let py = (character.y - self.scroll_y) as i32;
    
            for y in 0..character.metrics.height {
                for x in 0..character.metrics.width {
                    let val = character.bitmap[y * character.metrics.width + x];
                    let faded_val = ((val as u16 * alpha as u16) / 255) as u8;
    
                    let sx = px + x as i32;
                    let sy = py + y as i32;
    
                    if sx >= 0 && sx < width as i32 && sy >= 0 && sy < height as i32 {
                        let idx = ((sy as u32 * width as u32 + sx as u32) * 4) as usize;
                        buffer[idx + 0] = ((TEXT_COLOR.0 as u16 * faded_val as u16) / 255) as u8;
                        buffer[idx + 1] = ((TEXT_COLOR.1 as u16 * faded_val as u16) / 255) as u8;
                        buffer[idx + 2] = ((TEXT_COLOR.2 as u16 * faded_val as u16) / 255) as u8;
                        buffer[idx + 3] = faded_val;
                    }
                }
            }
        }
    
        // Get dimensions
        let width_u32 = width as u32;
        let height_u32 = height as u32;
        
        // Update button position (bottom right)
        let padding = 10;
        self.run_button.x = (width_u32 as i32) - (self.run_button.width as i32) - padding;
        self.run_button.y = (height_u32 as i32) - (self.run_button.height as i32) - padding;
        
        // Check if mouse is hovering over button
        let is_hovered = self.run_button.contains_point(mouse_x, mouse_y);
        
        // Draw cursor
        self.tick_cursor(buffer, width_u32, height_u32);
        
        // Draw run button
        self.run_button.draw(buffer, width_u32, height_u32, is_hovered);
    }

    fn on_scroll(&mut self, _state: &mut EngineState, _dx: f32, dy: f32) {
        self.scroll_y -= dy * 20.0;
        self.scroll_y = self.scroll_y.max(0.0);
    }

    fn on_key_char(&mut self, _state: &mut EngineState, ch: char) {
        match ch {
            '\t' => {
                // Insert tab at cursor position
                let text = &mut self.text_engine.text;
                let chars: Vec<char> = text.chars().collect();
                if self.cursor_position <= chars.len() {
                    let mut new_text = String::new();
                    for (i, c) in chars.iter().enumerate() {
                        if i == self.cursor_position {
                            new_text.push_str("    ");
                        }
                        new_text.push(*c);
                    }
                    if self.cursor_position == chars.len() {
                        new_text.push_str("    ");
                    }
                    self.text_engine.text = new_text;
                    self.cursor_position += 4;
                }
            }
            '\r' | '\n' => {
                // Insert newline at cursor position
                let text = &mut self.text_engine.text;
                let chars: Vec<char> = text.chars().collect();
                if self.cursor_position <= chars.len() {
                    let mut new_text = String::new();
                    for (i, c) in chars.iter().enumerate() {
                        if i == self.cursor_position {
                            new_text.push('\n');
                        }
                        new_text.push(*c);
                    }
                    if self.cursor_position == chars.len() {
                        new_text.push('\n');
                    }
                    self.text_engine.text = new_text;
                    self.cursor_position += 1;
                }
            }
            '\u{8}' => {
                // Backspace - delete character before cursor
                let text = &mut self.text_engine.text;
                if self.cursor_position > 0 {
                    let chars: Vec<char> = text.chars().collect();
                    let mut new_text = String::new();
                    for (i, c) in chars.iter().enumerate() {
                        if i != self.cursor_position - 1 {
                            new_text.push(*c);
                        }
                    }
                    self.text_engine.text = new_text;
                    self.cursor_position -= 1;
                }
            }
            _ => {
                if !ch.is_control() {
                    // Insert character at cursor position
                    let text = &mut self.text_engine.text;
                    let chars: Vec<char> = text.chars().collect();
                    if self.cursor_position <= chars.len() {
                        let mut new_text = String::new();
                        for (i, c) in chars.iter().enumerate() {
                            if i == self.cursor_position {
                                new_text.push(ch);
                            }
                            new_text.push(*c);
                        }
                        if self.cursor_position == chars.len() {
                            new_text.push(ch);
                        }
                        self.text_engine.text = new_text;
                        self.cursor_position += 1;
                    }
                }
            }
        }
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        let mouse_x = state.mouse.x;
        let mouse_y = state.mouse.y;
        
        // Check if click is on the run button
        if self.run_button.contains_point(mouse_x, mouse_y) {
            // Execute the Python code
            let code = self.text_engine.text.clone();
            if !code.trim().is_empty() {
                self.execute_python_code(&code);
            }
        }
    }
}

