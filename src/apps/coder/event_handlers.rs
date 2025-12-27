//! Event handling module for mouse and keyboard interactions

use crate::engine::EngineState;
use crate::engine::Application;
use super::types::{Tab, CodeViewMode};
use super::file_explorer::FileExplorer;
use super::python_runtime::PythonRuntime;
use crate::apps::text::text::TextApp;

pub struct EventHandlers {
    // Viewport double-tap tracking
    viewport_last_tap_time: Option<std::time::Instant>,
    viewport_last_tap_x: f32,
    viewport_last_tap_y: f32,
}

impl EventHandlers {
    pub fn new() -> Self {
        Self {
            viewport_last_tap_time: None,
            viewport_last_tap_x: 0.0,
            viewport_last_tap_y: 0.0,
        }
    }
    
    pub fn handle_scroll(&mut self, state: &mut EngineState, active_tab: Tab, file_explorer: &mut FileExplorer, code_app: &mut TextApp, terminal_app: &mut TextApp, dx: f32, dy: f32) {
        match active_tab {
            Tab::Code => {
                if file_explorer.view_mode == CodeViewMode::FileExplorer {
                    file_explorer.handle_scroll(dy);
                } else {
                    code_app.on_scroll(state, dx, dy);
                }
            }
            Tab::Terminal => terminal_app.on_scroll(state, dx, dy),
            Tab::Viewport => {
                // No scrolling in viewport
            }
        }
    }
    
    pub fn handle_key_char(&mut self, state: &mut EngineState, active_tab: Tab, file_explorer: &FileExplorer, code_app: &mut TextApp, console_app: &mut TextApp, runtime: &mut PythonRuntime, terminal_app: &mut TextApp, ch: char) {
        match active_tab {
            Tab::Code => {
                if file_explorer.view_mode == CodeViewMode::Editor {
                    code_app.on_key_char(state, ch);
                }
            }
            Tab::Terminal => {
                // Execute on Enter
                if ch == '\n' || ch == '\r' {
                    let command = console_app.text_rasterizer.text.clone();
                    let result = runtime.execute_console_command(&command);
                    
                    // Append to terminal
                    if !terminal_app.text_rasterizer.text.is_empty() {
                        terminal_app.text_rasterizer.text.push_str("\n");
                    }
                    terminal_app.text_rasterizer.text.push_str(">>> ");
                    terminal_app.text_rasterizer.text.push_str(command.split('\n').last().unwrap_or("").trim());
                    terminal_app.text_rasterizer.text.push_str("\n");
                    
                    match result {
                        Ok((output, _)) => {
                            if !output.is_empty() {
                                terminal_app.text_rasterizer.text.push_str(&output);
                            }
                        }
                        Err((error, _)) => {
                            terminal_app.text_rasterizer.text.push_str(&error);
                            terminal_app.text_rasterizer.text.push_str("\n");
                        }
                    }
                    
                    console_app.text_rasterizer.text.clear();
                    console_app.cursor_position = 0;
                    return;
                }
                
                console_app.on_key_char(state, ch);
            }
            Tab::Viewport => {
                // No keyboard input in viewport
            }
        }
    }
    
    pub fn handle_mouse_move(&mut self, state: &mut EngineState, active_tab: Tab, file_explorer: &mut FileExplorer, code_app: &mut TextApp, terminal_app: &mut TextApp, runtime: &mut PythonRuntime) {
        match active_tab {
            Tab::Code => {
                if file_explorer.view_mode == CodeViewMode::Editor {
                    code_app.on_mouse_move(state);
                } else if file_explorer.view_mode == CodeViewMode::FileExplorer {
                    if file_explorer.check_drag_threshold(state.mouse.x, state.mouse.y) || file_explorer.dragging {
                        file_explorer.update_drag(state.mouse.y);
                    }
                }
            }
            Tab::Terminal => terminal_app.on_mouse_move(state),
            Tab::Viewport => {
                if let Some(ref app_instance) = runtime.viewport_app {
                    let mouse_x = state.mouse.x;
                    let mouse_y = state.mouse.y;
                    runtime.interpreter.enter(|vm| {
                        let _ = vm.call_method(app_instance, "on_mouse_move", (mouse_x, mouse_y));
                    });
                }
            }
        }
    }
    
    pub fn handle_mouse_down(&mut self, state: &mut EngineState, active_tab: &mut Tab, file_explorer: &mut FileExplorer, code_app: &mut TextApp, terminal_app: &mut TextApp, console_app: &mut TextApp, runtime: &mut PythonRuntime, 
        run_button_clicked: bool, stop_button_clicked: bool, clear_button_clicked: bool, 
        code_tab_clicked: bool, terminal_tab_clicked: bool, viewport_tab_clicked: bool) {
        
        let mouse_x = state.mouse.x;
        let mouse_y = state.mouse.y;
        
        println!("Mouse down at ({}, {})", mouse_x, mouse_y);
        
        // Handle tab clicks
        if code_tab_clicked {
            println!("Code tab clicked");
            if *active_tab == Tab::Code {
                file_explorer.save_content(code_app.text_rasterizer.text.clone());
                file_explorer.toggle_view_mode();
                println!("Toggled view mode to {:?}", file_explorer.view_mode);
            } else {
                *active_tab = Tab::Code;
                file_explorer.view_mode = CodeViewMode::Editor;
            }
            return;
        }
        
        if terminal_tab_clicked {
            println!("Terminal tab clicked");
            *active_tab = Tab::Terminal;
            return;
        }
        
        if viewport_tab_clicked {
            println!("Viewport tab clicked");
            *active_tab = Tab::Viewport;
            return;
        }
        
        // Handle button clicks
        if clear_button_clicked {
            println!("Clear button clicked");
            console_app.text_rasterizer.text.clear();
            console_app.cursor_position = 0;
            return;
        }
        
        if stop_button_clicked {
            println!("Stop button clicked");
            let message = runtime.stop_execution();
            if let Ok(mut buffer) = runtime.output_buffer.lock() {
                buffer.push_str(&message);
            }
            return;
        }
        
        if run_button_clicked {
            println!("Run button clicked");
            let show_console = *active_tab == Tab::Terminal;
            let console_has_text = !console_app.text_rasterizer.text.trim().is_empty();
            
            if show_console && console_has_text {
                println!("Executing console command");
                let command = console_app.text_rasterizer.text.clone();
                let result = runtime.execute_console_command(&command);
                
                // Append to terminal
                if !terminal_app.text_rasterizer.text.is_empty() {
                    terminal_app.text_rasterizer.text.push_str("\n");
                }
                terminal_app.text_rasterizer.text.push_str(">>> ");
                terminal_app.text_rasterizer.text.push_str(command.split('\n').last().unwrap_or("").trim());
                terminal_app.text_rasterizer.text.push_str("\n");
                
                match result {
                    Ok((output, _)) => {
                        if !output.is_empty() {
                            terminal_app.text_rasterizer.text.push_str(&output);
                        }
                    }
                    Err((error, _)) => {
                        terminal_app.text_rasterizer.text.push_str(&error);
                        terminal_app.text_rasterizer.text.push_str("\n");
                    }
                }
                
                console_app.text_rasterizer.text.clear();
                console_app.cursor_position = 0;
            } else {
                let code = code_app.text_rasterizer.text.clone();
                println!("Executing code");
                if !code.trim().is_empty() {
                    use super::python_runtime::ExecutionResult;
                    match runtime.execute_code(&code) {
                        ExecutionResult::ViewportSuccess(msg) => {
                            terminal_app.text_rasterizer.text = msg;
                            if *active_tab == Tab::Code {
                                *active_tab = Tab::Viewport;
                            }
                        }
                        ExecutionResult::BackgroundStarted => {
                            terminal_app.text_rasterizer.text = "Running...\n".to_string();
                            if *active_tab == Tab::Code {
                                *active_tab = Tab::Terminal;
                            }
                        }
                        ExecutionResult::Error(error) => {
                            terminal_app.text_rasterizer.text = error;
                            if *active_tab == Tab::Code {
                                *active_tab = Tab::Terminal;
                            }
                        }
                    }
                }
            }
            return;
        }
        
        // Handle file explorer tap
        if *active_tab == Tab::Code && file_explorer.view_mode == CodeViewMode::FileExplorer {
            file_explorer.start_drag(mouse_x, mouse_y);
            return;
        }
        
        // Delegate to active app
        match *active_tab {
            Tab::Code => {
                if file_explorer.view_mode == CodeViewMode::Editor {
                    code_app.on_mouse_down(state);
                }
            }
            Tab::Terminal => terminal_app.on_mouse_down(state),
            Tab::Viewport => {
                self.handle_viewport_tap(state, runtime);
            }
        }
    }
    
    pub fn handle_mouse_up(&mut self, state: &mut EngineState, active_tab: Tab, file_explorer: &mut FileExplorer, code_app: &mut TextApp, terminal_app: &mut TextApp, runtime: &mut PythonRuntime, code_tab_label: &mut crate::text::text_rasterization::TextRasterizer) {
        match active_tab {
            Tab::Code => {
                if file_explorer.view_mode == CodeViewMode::Editor {
                    code_app.on_mouse_up(state);
                } else if file_explorer.view_mode == CodeViewMode::FileExplorer {
                    let shape = state.frame.array.shape();
                    let height = shape[0] as f32;
                    let safe_region_top_y = state.frame.safe_region_boundaries.y1 * height;
                    let (_, keyboard_top_y, _, _) = state.keyboard.onscreen.top_edge_coordinates();
                    let keyboard_top_px = keyboard_top_y * height;
                    let padding = 10;
                    let (_, button_height) = super::types::get_button_size();
                    let tabs_bottom_y = keyboard_top_px - padding as f32;
                    let tabs_top_y = tabs_bottom_y - button_height as f32;
                    
                    if let Some(clicked_index) = file_explorer.handle_tap(state.mouse.y, safe_region_top_y, tabs_top_y) {
                        println!("Selected file: {}", file_explorer.files[clicked_index].name);
                        if let Some((content, name)) = file_explorer.load_file(clicked_index) {
                            code_app.text_rasterizer.text = content;
                            code_app.cursor_position = 0;
                            code_app.scroll_y = 0.0;
                            code_tab_label.set_text(name);
                        }
                    }
                }
            }
            Tab::Terminal => terminal_app.on_mouse_up(state),
            Tab::Viewport => {
                if let Some(ref app_instance) = runtime.viewport_app {
                    let mouse_x = state.mouse.x;
                    let mouse_y = state.mouse.y;
                    runtime.interpreter.enter(|vm| {
                        let _ = vm.call_method(app_instance, "on_mouse_up", (mouse_x, mouse_y));
                    });
                }
            }
        }
    }
    
    fn handle_viewport_tap(&mut self, state: &EngineState, runtime: &mut PythonRuntime) -> bool {
        use std::time::{Duration, Instant};
        const DOUBLE_TAP_TIME_MS: u64 = 300;
        const DOUBLE_TAP_DISTANCE: f32 = 50.0;
        
        let mouse_x = state.mouse.x;
        let mouse_y = state.mouse.y;
        let now = Instant::now();
        
        let is_double_tap = if let Some(last_time) = self.viewport_last_tap_time {
            let time_since_last = now.duration_since(last_time);
            let distance = ((mouse_x - self.viewport_last_tap_x).powi(2) + (mouse_y - self.viewport_last_tap_y).powi(2)).sqrt();
            
            time_since_last < Duration::from_millis(DOUBLE_TAP_TIME_MS) && distance < DOUBLE_TAP_DISTANCE
        } else {
            false
        };
        
        if is_double_tap {
            self.viewport_last_tap_time = None;
            return true; // Signal to toggle keyboard
        } else {
            self.viewport_last_tap_time = Some(now);
            self.viewport_last_tap_x = mouse_x;
            self.viewport_last_tap_y = mouse_y;
            
            if let Some(ref app_instance) = runtime.viewport_app {
                runtime.interpreter.enter(|vm| {
                    let _ = vm.call_method(app_instance, "on_mouse_down", (mouse_x, mouse_y));
                });
            }
            
            false
        }
    }
    
    pub fn handle_viewport_mouse_down(&mut self, state: &EngineState, runtime: &mut PythonRuntime) -> bool {
        self.handle_viewport_tap(state, runtime)
    }
}

pub fn tab_contains_point(x: f32, y: f32, tab_x: i32, tab_y: i32, tab_width: u32, tab_height: u32) -> bool {
    x >= tab_x as f32 
        && x < (tab_x + tab_width as i32) as f32
        && y >= tab_y as f32
        && y < (tab_y + tab_height as i32) as f32
}

