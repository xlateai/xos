//! File explorer module for browsing and selecting Python files

use crate::text::text_rasterization::TextRasterizer;
use super::types::{PythonFile, CodeViewMode};
use include_dir::{include_dir, Dir};

// Embed the entire python/ directory at compile time
static PYTHON_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/python");

pub struct FileExplorer {
    pub files: Vec<PythonFile>,
    pub current_file_index: usize,
    pub scroll_y: f32,
    pub rasterizers: Vec<TextRasterizer>,
    pub view_mode: CodeViewMode,
    // Dragging state
    pub dragging: bool,
    pub last_mouse_y: f32,
    pub last_tap_x: f32,
    pub last_tap_y: f32,
}

impl FileExplorer {
    pub fn new(font: fontdue::Font) -> Self {
        let mut files = Vec::new();
        
        fn collect_py_files(dir: &Dir, base_path: &str, files: &mut Vec<PythonFile>) {
            // Collect all .py files in this directory
            for file in dir.files() {
                if let Some(filename) = file.path().file_name() {
                    if filename.to_string_lossy().ends_with(".py") {
                        let relative_path = if base_path.is_empty() {
                            filename.to_string_lossy().to_string()
                        } else {
                            format!("{}/{}", base_path, filename.to_string_lossy())
                        };
                        
                        if let Ok(content) = std::str::from_utf8(file.contents()) {
                            files.push(PythonFile {
                                name: relative_path.clone(),
                                content: content.to_string(),
                                path: relative_path,
                            });
                        }
                    }
                }
            }
            
            // Recursively collect from subdirectories
            for subdir in dir.dirs() {
                if let Some(dirname) = subdir.path().file_name() {
                    let new_base = if base_path.is_empty() {
                        dirname.to_string_lossy().to_string()
                    } else {
                        format!("{}/{}", base_path, dirname.to_string_lossy())
                    };
                    collect_py_files(subdir, &new_base, files);
                }
            }
        }
        
        collect_py_files(&PYTHON_DIR, "", &mut files);
        
        // Sort by name for consistent ordering
        files.sort_by(|a, b| a.name.cmp(&b.name));
        
        // Ensure we have at least one file
        if files.is_empty() {
            files.push(PythonFile {
                name: "empty.py".to_string(),
                content: "# No Python files found\nprint('Hello, World!')".to_string(),
                path: "empty.py".to_string(),
            });
        }
        
        // Create text rasterizers for each file in the file explorer
        // Larger font size for better touch targets on mobile (30% bigger on iOS)
        let file_list_font_size = if cfg!(target_os = "ios") { 42.0 } else { 24.0 };
        let mut rasterizers = Vec::new();
        for file in &files {
            let mut rasterizer = TextRasterizer::new(font.clone(), file_list_font_size);
            rasterizer.set_text(file.name.clone());
            rasterizers.push(rasterizer);
        }
        
        Self {
            files,
            current_file_index: 0,
            scroll_y: 0.0,
            rasterizers,
            view_mode: CodeViewMode::Editor,
            dragging: false,
            last_mouse_y: 0.0,
            last_tap_x: 0.0,
            last_tap_y: 0.0,
        }
    }
    
    pub fn get_current_file_content(&self) -> String {
        self.files[self.current_file_index].content.clone()
    }
    
    pub fn get_current_file_name(&self) -> String {
        self.files[self.current_file_index].name.clone()
    }
    
    pub fn save_content(&mut self, content: String) {
        if self.current_file_index < self.files.len() {
            self.files[self.current_file_index].content = content;
        }
    }
    
    pub fn load_file(&mut self, index: usize) -> Option<(String, String)> {
        if index < self.files.len() {
            self.current_file_index = index;
            self.view_mode = CodeViewMode::Editor;
            Some((
                self.files[index].content.clone(),
                self.files[index].name.clone(),
            ))
        } else {
            None
        }
    }
    
    pub fn toggle_view_mode(&mut self) {
        self.view_mode = match self.view_mode {
            CodeViewMode::Editor => CodeViewMode::FileExplorer,
            CodeViewMode::FileExplorer => CodeViewMode::Editor,
        };
    }
    
    pub fn handle_scroll(&mut self, dy: f32) {
        self.scroll_y += dy;
        // Clamp to valid range (use platform-specific item height, 30% bigger on iOS)
        let item_height = if cfg!(target_os = "ios") { 117.0 } else { 60.0 };
        let max_scroll = (self.files.len() as f32 * item_height).max(0.0);
        self.scroll_y = self.scroll_y.max(0.0).min(max_scroll);
    }
    
    pub fn start_drag(&mut self, x: f32, y: f32) {
        self.last_tap_x = x;
        self.last_tap_y = y;
        self.dragging = false;
    }
    
    pub fn update_drag(&mut self, mouse_y: f32) {
        if self.dragging {
            let dy = mouse_y - self.last_mouse_y;
            self.scroll_y -= dy;
            
            // Clamp to valid range
            let item_height = if cfg!(target_os = "ios") { 117.0 } else { 60.0 };
            let max_scroll = (self.files.len() as f32 * item_height).max(0.0);
            self.scroll_y = self.scroll_y.max(0.0).min(max_scroll);
            
            self.last_mouse_y = mouse_y;
        }
    }
    
    pub fn check_drag_threshold(&mut self, x: f32, y: f32) -> bool {
        if !self.dragging {
            let dx = (x - self.last_tap_x).abs();
            let dy = (y - self.last_tap_y).abs();
            if dx > 5.0 || dy > 5.0 {
                self.dragging = true;
                self.last_mouse_y = y;
                return true;
            }
        }
        false
    }
    
    pub fn handle_tap(&mut self, mouse_y: f32, safe_region_top_y: f32, tabs_top_y: f32) -> Option<usize> {
        let dy = (mouse_y - self.last_tap_y).abs();
        let drag_threshold = 10.0;
        
        if !self.dragging && dy < drag_threshold {
            if mouse_y >= safe_region_top_y && mouse_y < tabs_top_y {
                let item_height = if cfg!(target_os = "ios") { 117.0 } else { 60.0 };
                let clicked_index = ((mouse_y - safe_region_top_y + self.scroll_y) / item_height) as usize;
                if clicked_index < self.files.len() {
                    return Some(clicked_index);
                }
            }
        }
        
        self.dragging = false;
        None
    }
    
    pub fn draw(&self, buffer: &mut [u8], canvas_width: u32, canvas_height: u32, viewport_height: f32, safe_region_top_y: f32) {
        // Draw pitch black background for file list
        let bg_color = (0, 0, 0);
        
        // Start drawing from safe region top (below Dynamic Island on iOS)
        let start_y = safe_region_top_y as i32;
        
        for y in start_y..(viewport_height as i32) {
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
        
        // Larger touch targets for iOS (30% bigger)
        let item_height = if cfg!(target_os = "ios") { 117.0 } else { 60.0 };
        let padding = if cfg!(target_os = "ios") { 26.0 } else { 10.0 };
        
        for (i, rasterizer) in self.rasterizers.iter().enumerate() {
            let y_offset = safe_region_top_y + i as f32 * item_height - self.scroll_y;
            
            // Skip if not visible (check against safe region boundaries)
            if y_offset + item_height < safe_region_top_y || y_offset > viewport_height {
                continue;
            }
            
            // Draw item background (highlight if current file)
            let item_bg_color = if i == self.current_file_index {
                (30, 30, 30) // Slightly lighter gray for active file
            } else {
                (15, 15, 15) // Slightly off-black for file items
            };
            
            for dy in 0..(item_height as i32) {
                let y = y_offset as i32 + dy;
                if y >= safe_region_top_y as i32 && y < viewport_height as i32 {
                    for x in 0..(canvas_width as i32) {
                        let idx = ((y as u32 * canvas_width + x as u32) * 4) as usize;
                        buffer[idx + 0] = item_bg_color.0;
                        buffer[idx + 1] = item_bg_color.1;
                        buffer[idx + 2] = item_bg_color.2;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
            
            // Draw file name text using TextRasterizer
            let text_color = if i == self.current_file_index {
                (0, 255, 0) // Neon green for active file
            } else {
                (220, 220, 220) // Brighter text against pitch black
            };
            let text_y_offset = y_offset + (item_height - rasterizer.font_size) / 2.0;
            
            for character in &rasterizer.characters {
                let char_x = padding + character.x;
                let char_y = text_y_offset + character.y;
                
                for (bitmap_y, row) in character.bitmap.chunks(character.width as usize).enumerate() {
                    for (bitmap_x, &alpha) in row.iter().enumerate() {
                        if alpha == 0 {
                            continue;
                        }
                        
                        let px = (char_x + bitmap_x as f32) as i32;
                        let py = (char_y + bitmap_y as f32) as i32;
                        
                        if px >= 0 && px < canvas_width as i32 && py >= safe_region_top_y as i32 && py < viewport_height as i32 {
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
            
            // Draw separator line
            let separator_y = (y_offset + item_height - 1.0) as i32;
            if separator_y >= safe_region_top_y as i32 && separator_y < viewport_height as i32 {
                for x in 0..(canvas_width as i32) {
                    let idx = ((separator_y as u32 * canvas_width + x as u32) * 4) as usize;
                    buffer[idx + 0] = 30;
                    buffer[idx + 1] = 30;
                    buffer[idx + 2] = 30;
                    buffer[idx + 3] = 0xff;
                }
            }
        }
    }
    
    pub fn tick(&mut self, width: f32, height: f32) {
        for rasterizer in &mut self.rasterizers {
            rasterizer.tick(width, height);
        }
    }
}

