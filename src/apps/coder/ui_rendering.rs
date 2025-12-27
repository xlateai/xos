//! UI rendering module for tabs, buttons, console, and viewport

use crate::text::text_rasterization::TextRasterizer;
use crate::engine::EngineState;
use crate::apps::coder::button::Button;
use super::python_runtime::PythonRuntime;
use crate::apps::text::text::TextApp;
use rustpython_vm::AsObject;

pub struct UIRenderer {
    pub code_tab_label: TextRasterizer,
    pub terminal_tab_label: TextRasterizer,
    pub viewport_tab_label: TextRasterizer,
    pub clear_button_label: TextRasterizer,
}

impl UIRenderer {
    pub fn new(font: fontdue::Font, initial_filename: String) -> Self {
        let mut code_tab_label = TextRasterizer::new(font.clone(), 20.0);
        code_tab_label.set_text(initial_filename);
        
        let mut terminal_tab_label = TextRasterizer::new(font.clone(), 20.0);
        terminal_tab_label.set_text("terminal".to_string());
        
        let mut viewport_tab_label = TextRasterizer::new(font.clone(), 20.0);
        viewport_tab_label.set_text("viewport".to_string());
        
        let mut clear_button_label = TextRasterizer::new(font.clone(), 30.0);
        clear_button_label.set_text("×".to_string());
        
        Self {
            code_tab_label,
            terminal_tab_label,
            viewport_tab_label,
            clear_button_label,
        }
    }
    
    pub fn update_code_tab_label(&mut self, filename: String) {
        self.code_tab_label.set_text(filename);
    }
    
    #[allow(dead_code)]
    pub fn tick(&mut self, width: f32, height: f32) {
        self.code_tab_label.tick(width, height);
        self.terminal_tab_label.tick(width, height);
        self.viewport_tab_label.tick(width, height);
        self.clear_button_label.tick(width, height);
    }
    
    pub fn draw_tab(&self, buffer: &mut [u8], canvas_width: u32, canvas_height: u32, x: i32, y: i32, width: u32, height: u32, label_rasterizer: &TextRasterizer, is_active: bool) {
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
        draw_rect_border(buffer, canvas_width, canvas_height, x, y, width, height, text_color, !is_active);
        
        // Draw label text centered
        draw_centered_text(buffer, canvas_width, canvas_height, x, y, width, height, label_rasterizer, text_color);
    }
    
    pub fn draw_console(&self, buffer: &mut [u8], width: u32, height: u32, console_app: &TextApp, console_top_y: f32, console_bottom_y: f32) {
        // Draw console background
        let console_bg_color = (20, 20, 20);
        for y in (console_top_y as i32)..(console_bottom_y as i32) {
            if y >= 0 && y < height as i32 {
                for x in 0..(width as i32) {
                    let idx = ((y as u32 * width + x as u32) * 4) as usize;
                    buffer[idx + 0] = console_bg_color.0;
                    buffer[idx + 1] = console_bg_color.1;
                    buffer[idx + 2] = console_bg_color.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }
        
        // Draw console text
        let text_color = (0, 255, 0);
        for character in &console_app.text_rasterizer.characters {
            let px = character.x as i32;
            let py = (console_top_y + character.y - console_app.scroll_y) as i32;
            
            for y in 0..character.metrics.height {
                for x in 0..character.metrics.width {
                    let val = character.bitmap[y * character.metrics.width + x];
                    
                    let sx = px + x as i32;
                    let sy = py + y as i32;
                    
                    if sx >= 0 && sx < width as i32 && sy >= console_top_y as i32 && sy < console_bottom_y as i32 {
                        let idx = ((sy as u32 * width + sx as u32) * 4) as usize;
                        buffer[idx + 0] = ((text_color.0 as u16 * val as u16) / 255) as u8;
                        buffer[idx + 1] = ((text_color.1 as u16 * val as u16) / 255) as u8;
                        buffer[idx + 2] = ((text_color.2 as u16 * val as u16) / 255) as u8;
                        buffer[idx + 3] = val;
                    }
                }
            }
        }
        
        // Draw console cursor
        if console_app.show_cursor {
            draw_console_cursor(buffer, width, console_app, console_top_y, console_bottom_y, text_color);
        }
        
        // Draw console border
        let border_color = (100, 100, 100);
        for x in 0..(width as i32) {
            let y = console_top_y as i32;
            if y >= 0 && y < height as i32 {
                let idx = ((y as u32 * width + x as u32) * 4) as usize;
                buffer[idx + 0] = border_color.0;
                buffer[idx + 1] = border_color.1;
                buffer[idx + 2] = border_color.2;
                buffer[idx + 3] = 0xff;
            }
        }
    }
    
    pub fn draw_clear_button(&self, buffer: &mut [u8], width: u32, height: u32, clear_button: &Button, is_hovered: bool) {
        let text_color = if is_hovered {
            (180, 180, 180)
        } else {
            (120, 120, 120)
        };
        
        for character in &self.clear_button_label.characters {
            let text_width = self.clear_button_label.characters.iter()
                .map(|c| c.metrics.advance_width)
                .sum::<f32>();
            let text_offset_x = (clear_button.width as f32 - text_width) / 2.0;
            let text_offset_y = (clear_button.height as f32 - self.clear_button_label.font_size) / 2.0;
            
            let char_x = clear_button.x as f32 + character.x + text_offset_x;
            let char_y = clear_button.y as f32 + character.y + text_offset_y;
            
            for (bitmap_y, row) in character.bitmap.chunks(character.width as usize).enumerate() {
                for (bitmap_x, &alpha) in row.iter().enumerate() {
                    if alpha == 0 {
                        continue;
                    }
                    
                    let px = (char_x + bitmap_x as f32) as i32;
                    let py = (char_y + bitmap_y as f32) as i32;
                    
                    if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                        let idx = ((py as u32 * width + px as u32) * 4) as usize;
                        
                        let alpha_f = alpha as f32 / 255.0;
                        buffer[idx + 0] = ((text_color.0 as f32 * alpha_f) + (buffer[idx + 0] as f32 * (1.0 - alpha_f))) as u8;
                        buffer[idx + 1] = ((text_color.1 as f32 * alpha_f) + (buffer[idx + 1] as f32 * (1.0 - alpha_f))) as u8;
                        buffer[idx + 2] = ((text_color.2 as f32 * alpha_f) + (buffer[idx + 2] as f32 * (1.0 - alpha_f))) as u8;
                    }
                }
            }
        }
    }
    
    pub fn draw_button_with_color(button: &Button, buffer: &mut [u8], canvas_width: u32, canvas_height: u32, is_hovered: bool, color: (u8, u8, u8)) {
        let bg_color = if is_hovered {
            (
                (color.0 as u16 * 120 / 100).min(255) as u8,
                (color.1 as u16 * 120 / 100).min(255) as u8,
                (color.2 as u16 * 120 / 100).min(255) as u8,
            )
        } else {
            color
        };
        
        // Draw button background
        for dy in 0..button.height {
            for dx in 0..button.width {
                let px = button.x + dx as i32;
                let py = button.y + dy as i32;
                
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
        draw_rect_border(buffer, canvas_width, canvas_height, button.x, button.y, button.width, button.height, (255, 255, 255), true);
    }
    
    pub fn render_viewport_app(&self, runtime: &mut PythonRuntime, state: &mut EngineState) {
        if let Some(ref app_instance) = runtime.viewport_app {
            // Clear to black first
            let buffer = state.frame_buffer_mut();
            for i in (0..buffer.len()).step_by(4) {
                buffer[i + 0] = 0;
                buffer[i + 1] = 0;
                buffer[i + 2] = 0;
                buffer[i + 3] = 0xff;
            }
            
            // Setup if needed
            if !runtime.viewport_app_setup_done {
                let setup_result = runtime.interpreter.enter(|vm| {
                    let app_class_code = crate::python::engine::pyapp::APPLICATION_CLASS_CODE;
                    let scope = vm.new_scope_with_builtins();
                    if let Err(e) = vm.run_code_string(scope, app_class_code, "<viewport_setup>".to_string()) {
                        eprintln!("Failed to register Application class: {:?}", e);
                        return Err(());
                    }
                    
                    let frame_dict = crate::python::engine::py_bindings::create_py_frame_state(vm, &mut state.frame)
                        .map_err(|e| { eprintln!("Failed to create frame object: {:?}", e); () })?;
                    
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
                    
                    let mouse_dict = vm.ctx.new_dict();
                    let _ = mouse_dict.set_item("x", vm.ctx.new_float(state.mouse.x as f64).into(), vm);
                    let _ = mouse_dict.set_item("y", vm.ctx.new_float(state.mouse.y as f64).into(), vm);
                    let _ = mouse_dict.set_item("is_left_clicking", vm.ctx.new_bool(state.mouse.is_left_clicking).into(), vm);
                    app_instance.set_attr("mouse", mouse_dict, vm)
                        .map_err(|e| { eprintln!("Failed to set mouse attribute: {:?}", e); () })?;
                    
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
                    runtime.viewport_app_setup_done = true;
                }
            }
            
            // Tick the app
            if runtime.viewport_app_setup_done {
                let shape = state.frame.array.shape();
                let width = shape[1];
                let height = shape[0];
                let buffer = state.frame.buffer_mut();
                crate::python::rasterizer::set_frame_buffer_context(buffer, width, height);
                
                runtime.interpreter.enter(|vm| {
                    if let Ok(Some(frame_obj)) = vm.get_attribute_opt(app_instance.clone(), "frame") {
                        let _ = crate::python::engine::py_bindings::update_py_frame_state(vm, frame_obj.clone(), &mut state.frame);
                        
                        let mouse_dict = vm.ctx.new_dict();
                        let _ = mouse_dict.set_item("x", vm.ctx.new_float(state.mouse.x as f64).into(), vm);
                        let _ = mouse_dict.set_item("y", vm.ctx.new_float(state.mouse.y as f64).into(), vm);
                        let _ = mouse_dict.set_item("is_left_clicking", vm.ctx.new_bool(state.mouse.is_left_clicking).into(), vm);
                        let _ = app_instance.set_attr("mouse", mouse_dict, vm);
                        
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
                
                crate::python::rasterizer::clear_frame_buffer_context();
            }
        } else {
            // No app - show black screen
            let buffer = state.frame_buffer_mut();
            for i in (0..buffer.len()).step_by(4) {
                buffer[i + 0] = 0;
                buffer[i + 1] = 0;
                buffer[i + 2] = 0;
                buffer[i + 3] = 0xff;
            }
        }
    }
}

// Helper functions

fn draw_rect_border(buffer: &mut [u8], canvas_width: u32, canvas_height: u32, x: i32, y: i32, width: u32, height: u32, color: (u8, u8, u8), draw_bottom: bool) {
    // Top border
    for dx in 0..width {
        let px = x + dx as i32;
        if px >= 0 && px < canvas_width as i32 && y >= 0 && y < canvas_height as i32 {
            let idx = ((y as u32 * canvas_width + px as u32) * 4) as usize;
            buffer[idx + 0] = color.0;
            buffer[idx + 1] = color.1;
            buffer[idx + 2] = color.2;
            buffer[idx + 3] = 0xff;
        }
    }
    
    // Bottom border
    if draw_bottom {
        let bottom_y = y + height as i32 - 1;
        for dx in 0..width {
            let px = x + dx as i32;
            if px >= 0 && px < canvas_width as i32 && bottom_y >= 0 && bottom_y < canvas_height as i32 {
                let idx = ((bottom_y as u32 * canvas_width + px as u32) * 4) as usize;
                buffer[idx + 0] = color.0;
                buffer[idx + 1] = color.1;
                buffer[idx + 2] = color.2;
                buffer[idx + 3] = 0xff;
            }
        }
    }
    
    // Left border
    for dy in 0..height {
        let py = y + dy as i32;
        if py >= 0 && py < canvas_height as i32 && x >= 0 && x < canvas_width as i32 {
            let idx = ((py as u32 * canvas_width + x as u32) * 4) as usize;
            buffer[idx + 0] = color.0;
            buffer[idx + 1] = color.1;
            buffer[idx + 2] = color.2;
            buffer[idx + 3] = 0xff;
        }
    }
    
    // Right border
    let right_x = x + width as i32 - 1;
    for dy in 0..height {
        let py = y + dy as i32;
        if py >= 0 && py < canvas_height as i32 && right_x >= 0 && right_x < canvas_width as i32 {
            let idx = ((py as u32 * canvas_width + right_x as u32) * 4) as usize;
            buffer[idx + 0] = color.0;
            buffer[idx + 1] = color.1;
            buffer[idx + 2] = color.2;
            buffer[idx + 3] = 0xff;
        }
    }
}

fn draw_centered_text(buffer: &mut [u8], canvas_width: u32, canvas_height: u32, x: i32, y: i32, width: u32, height: u32, rasterizer: &TextRasterizer, text_color: (u8, u8, u8)) {
    for character in &rasterizer.characters {
        let text_width = rasterizer.characters.iter()
            .map(|c| c.metrics.advance_width)
            .sum::<f32>();
        let text_offset_x = (width as f32 - text_width) / 2.0;
        let text_offset_y = (height as f32 - rasterizer.font_size) / 2.0;
        
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
                    
                    let alpha_f = alpha as f32 / 255.0;
                    buffer[idx + 0] = ((text_color.0 as f32 * alpha_f) + (buffer[idx + 0] as f32 * (1.0 - alpha_f))) as u8;
                    buffer[idx + 1] = ((text_color.1 as f32 * alpha_f) + (buffer[idx + 1] as f32 * (1.0 - alpha_f))) as u8;
                    buffer[idx + 2] = ((text_color.2 as f32 * alpha_f) + (buffer[idx + 2] as f32 * (1.0 - alpha_f))) as u8;
                }
            }
        }
    }
}

fn draw_console_cursor(buffer: &mut [u8], width: u32, console_app: &TextApp, console_top_y: f32, console_bottom_y: f32, text_color: (u8, u8, u8)) {
    let line_info_with_idx = console_app.text_rasterizer.lines.iter()
        .enumerate()
        .find(|(_, line)| {
            line.start_index <= console_app.cursor_position && console_app.cursor_position <= line.end_index
        });
    
    let (cursor_x, baseline_y) = if let Some((line_idx, line)) = line_info_with_idx {
        let chars_in_line: Vec<_> = console_app.text_rasterizer.characters.iter()
            .filter(|c| c.line_index == line_idx)
            .collect();
        
        if chars_in_line.is_empty() || console_app.cursor_position == line.start_index {
            (0.0, line.baseline_y)
        } else if let Some(last_char) = chars_in_line.last() {
            if console_app.cursor_position > last_char.char_index {
                (last_char.x + last_char.metrics.advance_width, line.baseline_y)
            } else {
                (0.0, line.baseline_y)
            }
        } else {
            (0.0, line.baseline_y)
        }
    } else if let Some(first_line) = console_app.text_rasterizer.lines.first() {
        (0.0, first_line.baseline_y)
    } else {
        (0.0, console_app.text_rasterizer.ascent)
    };
    
    let cursor_top = (console_top_y + baseline_y - console_app.text_rasterizer.ascent - console_app.scroll_y).round() as i32;
    let cursor_bottom = (console_top_y + baseline_y + console_app.text_rasterizer.descent - console_app.scroll_y).round() as i32;
    let cx = cursor_x.round() as i32;
    
    for y in cursor_top..cursor_bottom {
        if y >= console_top_y as i32 && y < console_bottom_y as i32 && cx >= 0 && cx < width as i32 {
            let idx = ((y as u32 * width + cx as u32) * 4) as usize;
            buffer[idx + 0] = text_color.0;
            buffer[idx + 1] = text_color.1;
            buffer[idx + 2] = text_color.2;
            buffer[idx + 3] = 0xff;
        }
    }
}

