use crate::engine::keyboard::shortcuts::ShortcutAction;
use crate::engine::{Application, EngineState};
use crate::apps::text::text::TextApp;
use crate::rasterizer::{fill, fill_rect_buffer};
use crate::ui::Button;
use crate::rasterizer::text::text_rasterization::TextRasterizer;
use rustpython_vm::{Interpreter, AsObject};
use include_dir::{include_dir, Dir};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::thread;
// Embed the entire example-scripts/ directory at compile time
static PYTHON_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/example-scripts");

/// Task bar depth, chrome buttons, and related spacing/fonts vs prior baseline.
const CODER_CHROME_SCALE: f32 = 1.3;
/// Bottom task-bar tab button width + label size (+10%).
const TASKBAR_TAB_SCALE: f32 = 1.1;
/// Editor tab strip height and tab label fonts vs prior baseline (much bigger).
const EDITOR_TAB_STRIP_SCALE: f32 = 2.0;
const MAX_OPEN_EDITOR_TABS: usize = 3;

#[derive(Debug, Clone)]
struct OpenEditorTab {
    file_index: usize,
    text: String,
    /// Higher = more recently focused; LRU replacement picks minimum.
    last_accessed_stamp: u64,
}

enum EditorBarHit {
    Select(usize),
    Close(usize),
}

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

/// File explorer row: either a folder header or a file
#[derive(Debug, Clone)]
enum ExplorerItem {
    Folder(String),
    File(usize),
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
    /// Current file name only (no path), shown on the run button.
    run_button_label: TextRasterizer,
    persistent_scope: Option<rustpython_vm::scope::Scope>,
    // Python app instance for viewport rendering
    viewport_app: Option<rustpython_vm::PyObjectRef>,
    viewport_app_setup_done: bool,
    // Viewport double-tap tracking
    viewport_last_tap_time: Option<std::time::Instant>,
    viewport_last_tap_x: f32,
    viewport_last_tap_y: f32,
    /// Up to [`MAX_OPEN_EDITOR_TABS`] buffers; active index is [`Self::active_editor_tab`].
    open_editor_tabs: Vec<OpenEditorTab>,
    active_editor_tab: usize,
    explorer_access_counter: u64,
    /// Modal file picker over the editor (opened from the "files" task bar tab).
    explorer_popup_open: bool,
    /// Last closed / LRU-displaced editor file indices (most recent last) for Ctrl+Shift+T.
    closed_editor_history: Vec<usize>,
    /// Keyboard selection cursor in the explorer popup (index into `explorer_items`).
    explorer_selected_item: Option<usize>,
    /// Labels for the top editor tab strip (paths).
    editor_tab_labels: Vec<TextRasterizer>,
    editor_tab_close_label: TextRasterizer,
    python_files: Vec<PythonFile>,
    explorer_items: Vec<ExplorerItem>,
    expanded_folders: HashSet<String>,
    file_list_scroll_y: f32,
    file_list_rasterizers: Vec<TextRasterizer>,
    // File explorer dragging
    file_explorer_dragging: bool,
    file_explorer_last_mouse_y: f32,
    file_explorer_last_tap_x: f32,
    file_explorer_last_tap_y: f32,
    // Background Python execution
    python_output_buffer: Arc<Mutex<String>>,
    python_thread_handle: Option<thread::JoinHandle<()>>,
    python_thread_running: Arc<Mutex<bool>>,
    python_thread_generation: Arc<Mutex<u64>>, // Incremented each run; old threads ignored
    /// Skip `apply_coder_ui_scale` when scale is unchanged (avoids scanning explorer rasterizers).
    last_applied_ui_scale: f32,
}

impl CoderApp {
    fn file_basename(path: &str) -> String {
        path.rsplit('/').next().unwrap_or(path).to_string()
    }

    fn editor_tab_bar_height_px(ui_scale: f32) -> f32 {
        (40.0_f32 * ui_scale * CODER_CHROME_SCALE * EDITOR_TAB_STRIP_SCALE).max(32.0)
    }

    fn active_file_index(&self) -> Option<usize> {
        self.open_editor_tabs
            .get(self.active_editor_tab)
            .map(|t| t.file_index)
    }

    fn sync_editor_buffer_to_active_tab(&mut self) {
        if self.open_editor_tabs.is_empty() {
            return;
        }
        let idx = self.active_editor_tab;
        self.open_editor_tabs[idx].text = self.code_app.text_rasterizer.text.clone();
    }

    fn load_editor_from_active_tab(&mut self) {
        let t = &self.open_editor_tabs[self.active_editor_tab];
        self.code_app.text_rasterizer.text = t.text.clone();
        self.code_app.cursor_position = 0;
        self.code_app.scroll_y = 0.0;
        self.code_app.scroll_target = 0.0;
        self.code_app.clear_wheel_scroll_accel();
        if t.file_index < self.python_files.len() {
            self.run_button_label
                .set_text(Self::file_basename(&self.python_files[t.file_index].name));
        }
    }

    fn touch_editor_tab_lru(&mut self, tab_idx: usize) {
        self.explorer_access_counter = self.explorer_access_counter.saturating_add(1);
        self.open_editor_tabs[tab_idx].last_accessed_stamp = self.explorer_access_counter;
    }

    fn lru_editor_tab_index(&self) -> usize {
        self.open_editor_tabs
            .iter()
            .enumerate()
            .min_by_key(|(_, t)| t.last_accessed_stamp)
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    /// Open a file from the explorer, or focus an existing tab; respects [`MAX_OPEN_EDITOR_TABS`] with LRU replace.
    fn open_or_select_file_from_explorer(&mut self, file_index: usize) {
        if file_index >= self.python_files.len() {
            return;
        }
        self.sync_editor_buffer_to_active_tab();
        if let Some((i, _)) = self
            .open_editor_tabs
            .iter()
            .enumerate()
            .find(|(_, t)| t.file_index == file_index)
        {
            self.active_editor_tab = i;
            self.touch_editor_tab_lru(i);
            self.load_editor_from_active_tab();
            self.explorer_popup_open = false;
            return;
        }
        if self.open_editor_tabs.len() < MAX_OPEN_EDITOR_TABS {
            let text = self.python_files[file_index].content.clone();
            self.explorer_access_counter = self.explorer_access_counter.saturating_add(1);
            let stamp = self.explorer_access_counter;
            self.open_editor_tabs.push(OpenEditorTab {
                file_index,
                text,
                last_accessed_stamp: stamp,
            });
            self.active_editor_tab = self.open_editor_tabs.len() - 1;
        } else {
            let replace_idx = self.lru_editor_tab_index();
            let displaced = self.open_editor_tabs[replace_idx].file_index;
            self.closed_editor_history.push(displaced);
            self.explorer_access_counter = self.explorer_access_counter.saturating_add(1);
            let stamp = self.explorer_access_counter;
            self.open_editor_tabs[replace_idx] = OpenEditorTab {
                file_index,
                text: self.python_files[file_index].content.clone(),
                last_accessed_stamp: stamp,
            };
            self.active_editor_tab = replace_idx;
        }
        self.load_editor_from_active_tab();
        self.explorer_popup_open = false;
    }

    fn switch_editor_tab_to(&mut self, to: usize) {
        if to >= self.open_editor_tabs.len() || to == self.active_editor_tab {
            return;
        }
        self.sync_editor_buffer_to_active_tab();
        self.active_editor_tab = to;
        self.touch_editor_tab_lru(to);
        self.load_editor_from_active_tab();
    }

    fn switch_editor_tab_by(&mut self, delta: i32) {
        let n = self.open_editor_tabs.len();
        if n == 0 {
            return;
        }
        let cur = self.active_editor_tab as i32;
        let next = (cur + delta).rem_euclid(n as i32) as usize;
        self.switch_editor_tab_to(next);
    }

    fn switch_mode_tab_by(&mut self, delta: i32) {
        let order = [Tab::Code, Tab::Viewport, Tab::Terminal];
        let cur_idx = order
            .iter()
            .position(|t| *t == self.active_tab)
            .unwrap_or(0) as i32;
        let next = (cur_idx + delta).rem_euclid(order.len() as i32) as usize;
        self.active_tab = order[next];
        if self.active_tab != Tab::Code {
            self.explorer_popup_open = false;
        }
    }

    fn close_editor_tab_at(&mut self, index: usize) {
        if index >= self.open_editor_tabs.len() {
            return;
        }
        self.sync_editor_buffer_to_active_tab();
        let closed_fi = self.open_editor_tabs[index].file_index;
        self.closed_editor_history.push(closed_fi);

        if self.open_editor_tabs.len() == 1 {
            self.open_editor_tabs.clear();
            self.active_editor_tab = 0;
            self.code_app.text_rasterizer.text.clear();
            self.code_app.cursor_position = 0;
            self.code_app.scroll_y = 0.0;
            self.code_app.scroll_target = 0.0;
            self.code_app.clear_wheel_scroll_accel();
            self.run_button_label.set_text(String::new());
            return;
        }

        let was_active = index == self.active_editor_tab;
        self.open_editor_tabs.remove(index);
        if self.active_editor_tab >= self.open_editor_tabs.len() {
            self.active_editor_tab = self.open_editor_tabs.len().saturating_sub(1);
        } else if index < self.active_editor_tab {
            self.active_editor_tab -= 1;
        }
        if was_active {
            self.load_editor_from_active_tab();
        }
    }

    fn reopen_last_closed_editor_file(&mut self) {
        while let Some(fi) = self.closed_editor_history.pop() {
            if fi < self.python_files.len() {
                self.open_or_select_file_from_explorer(fi);
                return;
            }
        }
    }

    fn explorer_panel_bounds(
        width: f32,
        height: f32,
        safe_top_y: f32,
        bar_h: f32,
        task_bar_top: f32,
        ui_scale: f32,
    ) -> (f32, f32, f32, f32) {
        let panel_margin = 24.0_f32 * ui_scale.max(0.5);
        let px = panel_margin;
        let pw = width - 2.0 * panel_margin;
        let pt = safe_top_y + bar_h + panel_margin;
        let pb = (pt + height * 0.52_f32)
            .min(task_bar_top - panel_margin)
            .max(pt + 140.0);
        (px, pt, px + pw, pb)
    }

    fn editor_tab_bar_mouse_hit(
        &self,
        mouse_x: f32,
        mouse_y: f32,
        safe_top_y: f32,
        bar_h: f32,
        padding: i32,
        width: f32,
        ui_scale: f32,
    ) -> Option<EditorBarHit> {
        if mouse_y < safe_top_y || mouse_y >= safe_top_y + bar_h {
            return None;
        }
        let n = self.open_editor_tabs.len().max(1);
        let inner_w = width - 2.0 * padding as f32;
        let tab_w = if self.open_editor_tabs.is_empty() {
            inner_w
        } else {
            inner_w / n as f32
        };
        let close_reserve = (30.0_f32 * CODER_CHROME_SCALE * ui_scale.max(0.4)).max(22.0);
        for (i, _) in self.open_editor_tabs.iter().enumerate() {
            let x0 = padding as f32 + i as f32 * tab_w;
            let x1 = padding as f32 + (i + 1) as f32 * tab_w;
            if mouse_x >= x0 && mouse_x < x1 {
                let close_left = x1 - close_reserve;
                if mouse_x >= close_left {
                    return Some(EditorBarHit::Close(i));
                }
                return Some(EditorBarHit::Select(i));
            }
        }
        None
    }

    /// Scale chrome and editor text from the shorter window edge (design reference ~920px).
    fn ui_scale(short_edge: f32) -> f32 {
        const REF: f32 = 920.0;
        (short_edge / REF).clamp(0.28, 1.0)
    }

    /// Calibrate prior coder chrome (~75% vs text) to ~50% at default slider, with ~30% zoom-out.
    /// F3 scale uses [`crate::engine::f3_ui_scale_multiplier`] (same curve as standalone text).
    fn layout_scale_from_state(state: &EngineState) -> f32 {
        const CODER_UI_CALIBRATION: f32 = (50.0 / 75.0) * 0.7;
        let shape = state.frame.tensor.shape();
        Self::ui_scale((shape[1] as f32).min(shape[0] as f32))
            * state.f3_ui_scale_multiplier()
            * CODER_UI_CALIBRATION
    }

    fn button_size_scaled(scale: f32) -> (u32, u32) {
        #[cfg(target_os = "ios")]
        let (bw, bh) = (280.0_f32, 105.0_f32);
        #[cfg(not(target_os = "ios"))]
        let (bw, bh) = (160.0_f32, 76.0_f32);
        let w = (((bw * scale).max(44.0)) * CODER_CHROME_SCALE).round() as u32;
        let h = (((bh * scale).max(36.0)) * CODER_CHROME_SCALE).round() as u32;
        (w, h)
    }

    fn tab_width_scaled(scale: f32) -> u32 {
        let (button_width, _) = Self::button_size_scaled(scale);
        #[cfg(target_os = "ios")]
        {
            (button_width as f32 * 0.8 * TASKBAR_TAB_SCALE).round() as u32
        }
        #[cfg(not(target_os = "ios"))]
        {
            (button_width as f32 * TASKBAR_TAB_SCALE).round() as u32
        }
    }

    fn padding_scaled(scale: f32) -> i32 {
        ((10.0_f32 * scale).max(4.0) * CODER_CHROME_SCALE).round() as i32
    }

    fn apply_coder_ui_scale(&mut self, scale: f32) {
        if (self.last_applied_ui_scale - scale).abs() < 1e-4 {
            return;
        }
        self.last_applied_ui_scale = scale;
        let editor_base = if cfg!(target_os = "ios") {
            48.0 * 1.1
        } else {
            48.0
        };
        self.code_app.set_font_size(editor_base * scale);
        self.terminal_app.set_font_size(editor_base * scale);
        self.console_app.set_font_size(editor_base * scale);

        let chrome_font = CODER_CHROME_SCALE;
        let task_tab_font = chrome_font * TASKBAR_TAB_SCALE;
        self.code_tab_label.set_font_size(20.0 * scale * task_tab_font);
        self.terminal_tab_label
            .set_font_size(20.0 * scale * task_tab_font);
        self.viewport_tab_label
            .set_font_size(20.0 * scale * task_tab_font);
        self.clear_button_label.set_font_size(30.0 * scale * chrome_font);
        self.run_button_label.set_font_size(20.0 * scale * chrome_font);
        let tab_strip = chrome_font * EDITOR_TAB_STRIP_SCALE;
        for r in &mut self.editor_tab_labels {
            r.set_font_size(16.0 * scale * tab_strip);
        }
        self.editor_tab_close_label
            .set_font_size(20.0 * scale * tab_strip);

        let file_base = if cfg!(target_os = "ios") { 42.0 } else { 24.0 };
        let folder_base = if cfg!(target_os = "ios") { 32.0 } else { 18.0 };
        for (item, r) in self
            .explorer_items
            .iter()
            .zip(self.file_list_rasterizers.iter_mut())
        {
            let sz = match item {
                ExplorerItem::Folder(_) => folder_base * scale,
                ExplorerItem::File(_) => file_base * scale,
            };
            r.set_font_size(sz);
        }

        let (bw, bh) = Self::button_size_scaled(scale);
        self.run_button.width = bw;
        self.run_button.height = bh;
        self.stop_button.width = bh;
        self.stop_button.height = bh;
        self.clear_button.width = bh;
        self.clear_button.height = bh;
    }
    
    pub fn new() -> Self {
        // Enable coder logging to capture all Rust/Swift logs
        super::logging::enable_coder_logging();
        
        // Discover all Python files from the embedded directory
        let mut python_files = Vec::new();
        
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
        
        collect_py_files(&PYTHON_DIR, "", &mut python_files);
        
        // Sort by name for consistent ordering (groups files by folder)
        python_files.sort_by(|a, b| a.name.cmp(&b.name));
        
        // Build explorer items with folder headers when folder changes
        let mut explorer_items = Vec::new();
        let mut last_folder = None;
        for (i, file) in python_files.iter().enumerate() {
            let folder = file.name.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
            if last_folder.as_ref() != Some(&folder) {
                if !folder.is_empty() {
                    explorer_items.push(ExplorerItem::Folder(folder.clone()));
                }
                last_folder = Some(folder);
            }
            explorer_items.push(ExplorerItem::File(i));
        }
        
        // Ensure we have at least one file
        if python_files.is_empty() {
            python_files.push(PythonFile {
                name: "empty.py".to_string(),
                content: "# No Python files found\nprint('Hello, World!')".to_string(),
                path: "empty.py".to_string(),
            });
        }
        
        // Create the code editor text app (no file open until the user picks one).
        let mut code_app = TextApp::new();
        code_app.uses_parent_ui_scale = true;
        code_app.text_rasterizer.text = String::new();
        code_app.show_debug_visuals = false; // Hide debug visuals in code editor
        // On iOS, start at top like terminal. On macOS, start at top too.
        code_app.scroll_y = 0.0;
        code_app.scroll_target = 0.0;
        code_app.clear_wheel_scroll_accel();
        
        // Create the terminal text app (empty initially)
        let mut terminal_app = TextApp::new();
        terminal_app.uses_parent_ui_scale = true;
        terminal_app.text_rasterizer.text = "".to_string();
        terminal_app.read_only = true; // Terminal is read-only
        terminal_app.show_cursor = false; // Hide cursor in terminal
        terminal_app.show_debug_visuals = false; // Hide debug visuals in terminal
        
        // Create the console text app for interactive Python
        let mut console_app = TextApp::new();
        console_app.uses_parent_ui_scale = true;
        console_app.text_rasterizer.text = "".to_string();
        console_app.show_debug_visuals = false; // Hide debug visuals in console

        // Initialize RustPython interpreter with xos module
        let interpreter = Interpreter::with_init(Default::default(), |vm| {
            // Register the xos native module
            vm.add_native_module("xos".to_owned(), Box::new(crate::python_api::xos_module::make_module));
        });

        // Initialize persistent scope for console
        let persistent_scope = None; // Will be created on first use

        // Create run button (position will be updated in tick; initial size matches scale 1.0)
        let (button_width, button_height) = Self::button_size_scaled(1.0);
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
        let font_data = include_bytes!("../../assets/JetBrainsMono-Regular.ttf");
        let font = fontdue::Font::from_bytes(
            font_data as &[u8],
            fontdue::FontSettings::default(),
        ).expect("Failed to load font");
        
        // Create text rasterizers for tab labels
        let mut code_tab_label = TextRasterizer::new(font.clone(), 20.0 * TASKBAR_TAB_SCALE);
        code_tab_label.set_text("files".to_string());

        let mut run_button_label = TextRasterizer::new(font.clone(), 20.0);
        run_button_label.set_text(String::new());
        
        let mut terminal_tab_label = TextRasterizer::new(font.clone(), 20.0 * TASKBAR_TAB_SCALE);
        terminal_tab_label.set_text("terminal".to_string());
        
        let mut viewport_tab_label = TextRasterizer::new(font.clone(), 20.0 * TASKBAR_TAB_SCALE);
        viewport_tab_label.set_text("viewport".to_string());
        
        // Create text rasterizer for clear button "x" label
        let mut clear_button_label = TextRasterizer::new(font.clone(), 30.0);
        clear_button_label.set_text("×".to_string()); // Multiplication sign

        let mut editor_tab_labels = Vec::new();
        for _ in 0..MAX_OPEN_EDITOR_TABS {
            editor_tab_labels.push(TextRasterizer::new(
                font.clone(),
                14.0 * EDITOR_TAB_STRIP_SCALE,
            ));
        }
        let mut editor_tab_close_label =
            TextRasterizer::new(font.clone(), 20.0 * EDITOR_TAB_STRIP_SCALE);
        editor_tab_close_label.set_text("×".to_string());

        // Create text rasterizers for each explorer item (folder headers + files)
        let file_list_font_size = if cfg!(target_os = "ios") { 42.0 } else { 24.0 };
        let folder_font_size = if cfg!(target_os = "ios") { 32.0 } else { 18.0 };
        let mut file_list_rasterizers = Vec::new();
        let expanded_folders = HashSet::new(); // All collapsed by default
        for item in &explorer_items {
            let mut rasterizer = TextRasterizer::new(font.clone(), match item {
                ExplorerItem::Folder(_) => folder_font_size,
                ExplorerItem::File(_) => file_list_font_size,
            });
            let text = match item {
                ExplorerItem::Folder(name) => format!("  ▶  {}/", name), // ▶ = collapsed
                ExplorerItem::File(i) => python_files[*i].name.clone(),
            };
            rasterizer.set_text(text);
            file_list_rasterizers.push(rasterizer);
        }

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
            run_button_label,
            persistent_scope,
            viewport_app: None,
            viewport_app_setup_done: false,
            viewport_last_tap_time: None,
            viewport_last_tap_x: 0.0,
            viewport_last_tap_y: 0.0,
            open_editor_tabs: Vec::new(),
            active_editor_tab: 0,
            explorer_access_counter: 0,
            explorer_popup_open: false,
            closed_editor_history: Vec::new(),
            explorer_selected_item: None,
            editor_tab_labels,
            editor_tab_close_label,
            python_files,
            explorer_items,
            expanded_folders,
            file_list_scroll_y: 0.0,
            file_list_rasterizers,
            file_explorer_dragging: false,
            file_explorer_last_mouse_y: 0.0,
            file_explorer_last_tap_x: 0.0,
            file_explorer_last_tap_y: 0.0,
            python_output_buffer: Arc::new(Mutex::new(String::new())),
            python_thread_handle: None,
            python_thread_running: Arc::new(Mutex::new(false)),
            python_thread_generation: Arc::new(Mutex::new(0)),
            last_applied_ui_scale: f32::NAN,
        }
    }

    fn execute_python_code(&mut self, code: &str) {
        // Wait for any previous thread to complete
        if let Some(handle) = self.python_thread_handle.take() {
            // Mark as not running before waiting
            *self.python_thread_running.lock().unwrap() = false;
            let _ = handle.join();
        }
        
        // Clear terminal and output buffer
        self.terminal_app.text_rasterizer.text = "Running...\n".to_string();
        {
            let mut buffer = self.python_output_buffer.lock().unwrap();
            buffer.clear();
        }
        
        // Clear any previous viewport app before execution and clean up microphones
        self.viewport_app = None;
        self.viewport_app_setup_done = false;
        crate::python_api::audio::cleanup_all_audio();
        
        // Detect if this is a viewport app (contains xos.Application)
        let is_viewport_app = code.contains("xos.Application") || code.contains("class") && code.contains("Application");
        
        if is_viewport_app {
            // Execute in main thread for viewport apps (they use tick())
            self.execute_viewport_app(code);
        } else {
            // Execute in background thread for non-viewport scripts (e.g. print loops)
            self.execute_background_script(code);
        }
    }
    
    fn execute_viewport_app(&mut self, code: &str) {
        // Clear output buffer before execution
        {
            let mut buffer = self.python_output_buffer.lock().unwrap();
            buffer.clear();
        }
        
        // Execute with print capture, but DON'T restore print (keep it active for setup/tick)
        let (result, app_instance, new_scope) = self.interpreter.enter(|vm| {
            // Clear previous app instance
            let _ = vm.builtins.as_object().to_owned().del_attr("__xos_app_instance__", vm);
            
            // Get or create scope
            let scope = if let Some(ref existing_scope) = self.persistent_scope {
                existing_scope.clone()
            } else {
                let new_scope = vm.new_scope_with_builtins();
                let _ = new_scope.globals.set_item("__name__", vm.ctx.new_str("__main__").into(), vm);
                new_scope
            };
            
            // Set up persistent print capture (DON'T restore it)
            let buffer_clone = Arc::clone(&self.python_output_buffer);
            let write_output_fn = vm.new_function(
                "__write_output__",
                move |args: rustpython_vm::function::FuncArgs, _vm: &rustpython_vm::VirtualMachine| -> rustpython_vm::PyResult {
                    if let Some(text_obj) = args.args.first() {
                        if let Ok(text) = text_obj.str(_vm) {
                            if let Ok(mut buffer) = buffer_clone.lock() {
                                buffer.push_str(&text.to_string());
                            }
                        }
                    }
                    Ok(_vm.ctx.none())
                },
            );
            scope.globals.set_item("__write_output__", write_output_fn.into(), vm).ok();
            
            // Override print permanently (for setup/tick calls)
            let setup_code = r#"
import builtins
import xos

def __custom_print__(*args, sep=' ', end='\n', **kwargs):
    output = sep.join(str(arg) for arg in args) + end
    __write_output__(output)

builtins.print = __custom_print__
xos.print = __custom_print__
"#;
            
            if let Err(e) = vm.run_code_string(scope.clone(), setup_code, "<setup>".to_string()) {
                eprintln!("Failed to set up print capture: {:?}", e);
            }
            
            // Run the code
            let exec_result = vm.run_code_string(scope.clone(), code, "<coder>".to_string());
            
            // Handle errors
            let result = if let Err(py_exc) = exec_result {
                let error_text = crate::python_api::runtime::format_python_exception(vm, &py_exc);
                Err(error_text)
            } else {
                Ok(())
            };
            
            // Check for app instance
            let app_instance = vm.get_attribute_opt(vm.builtins.as_object().to_owned(), "__xos_app_instance__")
                .ok()
                .flatten();
            
            (result, app_instance, scope)
        });
        
        // Update persistent scope
        self.persistent_scope = Some(new_scope);
        
        // Get current output
        let current_output = self.python_output_buffer.lock().unwrap().clone();
        
        // Handle result
        match result {
            Ok(_) => {
                if let Some(app_instance_obj) = app_instance {
                    self.viewport_app = Some(app_instance_obj);
                    self.viewport_app_setup_done = false;
                    
                    // Keep the output buffer active - it will be updated by setup/tick
                    // Terminal will show accumulated output from the buffer
                    if self.active_tab == Tab::Code {
                        self.active_tab = Tab::Viewport;
                    }
                } else {
                    self.terminal_app.text_rasterizer.text = if !current_output.is_empty() { current_output } else { "(no output)".to_string() };
                }
            }
            Err(error) => {
                let mut error_display = current_output;
                if !error_display.trim().is_empty() {
                    error_display.push_str("\n");
                }
                error_display.push_str(&error);
                error_display.push('\n');
                
                self.terminal_app.text_rasterizer.text = error_display;
                if self.active_tab == Tab::Code {
                    self.active_tab = Tab::Terminal;
                }
            }
        }
    }
    
    fn execute_background_script(&mut self, code: &str) {
        // Clone what we need for the thread
        let code_str = code.to_string();
        let output_buffer = Arc::clone(&self.python_output_buffer);
        let running_flag = Arc::clone(&self.python_thread_running);
        let generation_counter = Arc::clone(&self.python_thread_generation);
        
        // Increment generation (invalidates any previous thread's output)
        let current_generation = {
            let mut gen = self.python_thread_generation.lock().unwrap();
            *gen += 1;
            *gen
        };
        
        // Mark thread as running
        *self.python_thread_running.lock().unwrap() = true;
        
        // Spawn background thread to execute Python
        let handle = thread::spawn(move || {
            // Create interpreter in this thread
            let interpreter = Interpreter::with_init(Default::default(), |vm| {
                vm.add_native_module("xos".to_owned(), Box::new(crate::python_api::xos_module::make_module));
            });
            
            // Create print callback that checks generation
            let buffer_for_callback = Arc::clone(&output_buffer);
            let gen_for_callback = Arc::clone(&generation_counter);
            let print_callback: crate::python_api::runtime::PrintCallback = Arc::new(move |text: &str| {
                if let Ok(current_gen) = gen_for_callback.lock() {
                    if *current_gen == current_generation {
                        if let Ok(mut buffer) = buffer_for_callback.lock() {
                            buffer.push_str(text);
                        }
                    }
                }
            });
            
            // Execute using unified runtime
            let (result, _, _, _) = crate::python_api::runtime::execute_python_code(
                &interpreter,
                &code_str,
                "<coder>",
                None,
                Some(print_callback),
            );
            
            // Handle errors (only if still current generation)
            if let Err(error_msg) = result {
                if let Ok(current_gen) = generation_counter.lock() {
                    if *current_gen == current_generation {
                        if let Ok(mut buffer) = output_buffer.lock() {
                            buffer.push_str("\n");
                            buffer.push_str(&error_msg);
                            buffer.push_str("\n");
                        }
                    }
                }
            }
            
            // Mark thread as no longer running (only if still current generation)
            if let Ok(current_gen) = generation_counter.lock() {
                if *current_gen == current_generation {
                    if let Ok(mut flag) = running_flag.lock() {
                        *flag = false;
                    }
                }
            }
        });
        
        // Store the thread handle
        self.python_thread_handle = Some(handle);
        
        // Switch to terminal tab to show output
        if self.active_tab == Tab::Code {
            self.active_tab = Tab::Terminal;
        }
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
        let cw = canvas_width as usize;
        let ch = canvas_height as usize;
        let x1 = x + width as i32;
        let y1 = y + height as i32;

        fill_rect_buffer(
            buffer,
            cw,
            ch,
            x,
            y,
            x1,
            y1,
            (bg_color.0, bg_color.1, bg_color.2, 0xff),
        );
        fill_rect_buffer(
            buffer,
            cw,
            ch,
            x,
            y,
            x1,
            y + 1,
            (text_color.0, text_color.1, text_color.2, 0xff),
        );
        if !is_active {
            let bottom_y = y1 - 1;
            fill_rect_buffer(
                buffer,
                cw,
                ch,
                x,
                bottom_y,
                x1,
                y1,
                (text_color.0, text_color.1, text_color.2, 0xff),
            );
        }
        fill_rect_buffer(
            buffer,
            cw,
            ch,
            x,
            y,
            x + 1,
            y1,
            (text_color.0, text_color.1, text_color.2, 0xff),
        );
        fill_rect_buffer(
            buffer,
            cw,
            ch,
            x1 - 1,
            y,
            x1,
            y1,
            (text_color.0, text_color.1, text_color.2, 0xff),
        );
        
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
            
            let cw = character.width as usize;
            if cw == 0 { continue; }
            for (bitmap_y, row) in character.bitmap.chunks(cw).enumerate() {
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

    fn save_current_file(&mut self) {
        self.sync_editor_buffer_to_active_tab();
        let Some(fi) = self.active_file_index() else {
            return;
        };
        if fi < self.python_files.len() {
            self.python_files[fi].content = self.open_editor_tabs[self.active_editor_tab]
                .text
                .clone();
        }
    }

    fn activate_run_button(&mut self, state: &mut EngineState) {
        // Mirror the run-button click behavior.
        let keyboard_is_shown = state.keyboard.onscreen.is_shown();
        let show_console = self.active_tab == Tab::Terminal;
        let console_has_text = !self.console_app.text_rasterizer.text.trim().is_empty();

        if keyboard_is_shown && show_console && console_has_text {
            let command = self.console_app.text_rasterizer.text.clone();
            self.execute_console_command(&command);
            self.console_app.text_rasterizer.text.clear();
            self.console_app.cursor_position = 0;
            return;
        }

        let code = self.code_app.text_rasterizer.text.clone();
        if !code.trim().is_empty() {
            self.execute_python_code(&code);
        }
    }
    
    fn file_item_height(scale: f32) -> f32 {
        let base = if cfg!(target_os = "ios") { 117.0 } else { 60.0 };
        (base * scale).max(24.0)
    }

    fn folder_item_height(scale: f32) -> f32 {
        let base = if cfg!(target_os = "ios") { 52.0 } else { 36.0 };
        (base * scale).max(20.0)
    }

    fn explorer_item_height(item: &ExplorerItem, scale: f32) -> f32 {
        match item {
            ExplorerItem::Folder(_) => Self::folder_item_height(scale),
            ExplorerItem::File(_) => Self::file_item_height(scale),
        }
    }
    
    /// Folder for a file (empty string = root level)
    fn folder_for_file(&self, file_idx: usize) -> String {
        self.python_files
            .get(file_idx)
            .and_then(|f| f.name.rsplit_once('/').map(|(d, _)| d.to_string()))
            .unwrap_or_default()
    }
    
    /// True if item is visible (folders always visible; files only when their folder is expanded)
    fn is_item_visible(&self, item: &ExplorerItem) -> bool {
        match item {
            ExplorerItem::Folder(_) => true,
            ExplorerItem::File(i) => {
                let folder = self.folder_for_file(*i);
                folder.is_empty() || self.expanded_folders.contains(&folder)
            }
        }
    }

    fn explorer_visible_indices(&self) -> Vec<usize> {
        self.explorer_items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| self.is_item_visible(item).then_some(i))
            .collect()
    }

    fn ensure_explorer_selection(&mut self) {
        let visible = self.explorer_visible_indices();
        if visible.is_empty() {
            self.explorer_selected_item = None;
            return;
        }

        if let Some(sel) = self.explorer_selected_item {
            if visible.contains(&sel) {
                return;
            }
        }

        // Prefer the currently open file if any.
        if let Some(active_fi) = self.active_file_index() {
            if let Some(idx) = self.explorer_items.iter().enumerate().find_map(|(i, it)| {
                matches!(it, ExplorerItem::File(fi) if *fi == active_fi).then_some(i)
            }) {
                if visible.contains(&idx) {
                    self.explorer_selected_item = Some(idx);
                    return;
                }
            }
        }

        self.explorer_selected_item = Some(visible[0]);
    }

    fn explorer_move_selection(&mut self, delta: i32) {
        let visible = self.explorer_visible_indices();
        if visible.is_empty() {
            self.explorer_selected_item = None;
            return;
        }
        self.ensure_explorer_selection();
        let Some(sel) = self.explorer_selected_item else {
            self.explorer_selected_item = Some(visible[0]);
            return;
        };
        let Some(pos) = visible.iter().position(|i| *i == sel) else {
            self.explorer_selected_item = Some(visible[0]);
            return;
        };
        let n = visible.len() as i32;
        let next = (pos as i32 + delta).clamp(0, n - 1) as usize;
        self.explorer_selected_item = Some(visible[next]);
    }

    fn explorer_activate_selected(&mut self) {
        self.ensure_explorer_selection();
        let Some(sel) = self.explorer_selected_item else {
            return;
        };
        match self.explorer_items.get(sel) {
            Some(ExplorerItem::Folder(name)) => {
                if self.expanded_folders.contains(name) {
                    self.expanded_folders.remove(name);
                    self.file_list_rasterizers[sel].set_text(format!("  ▶  {}/", name));
                } else {
                    self.expanded_folders.insert(name.clone());
                    self.file_list_rasterizers[sel].set_text(format!("  ▼  {}/", name));
                }
                self.ensure_explorer_selection();
            }
            Some(ExplorerItem::File(file_index)) => {
                self.open_or_select_file_from_explorer(*file_index);
            }
            None => {}
        }
    }
    
    fn draw_file_explorer(
        &self,
        buffer: &mut [u8],
        canvas_width: u32,
        canvas_height: u32,
        top_y: f32,
        bottom_y: f32,
        clip_x0: i32,
        clip_x1: i32,
        ui_scale: f32,
    ) {
        let cw = canvas_width as usize;
        let ch = canvas_height as usize;
        let start_y = top_y as i32;
        let end_y = bottom_y as i32;
        fill_rect_buffer(
            buffer,
            cw,
            ch,
            clip_x0,
            start_y,
            clip_x1,
            end_y,
            (0, 0, 0, 0xff),
        );
        
        let padding_base = if cfg!(target_os = "ios") { 26.0 } else { 10.0 };
        let padding = (padding_base * ui_scale).max(4.0);

        let mut y_offset = top_y - self.file_list_scroll_y;
        let active_fi = self.active_file_index();

        for (_i, (item, rasterizer)) in self
            .explorer_items
            .iter()
            .zip(self.file_list_rasterizers.iter())
            .enumerate()
        {
            if !self.is_item_visible(item) {
                continue; // Skip collapsed files
            }
            let item_height = CoderApp::explorer_item_height(item, ui_scale);
            
            if y_offset + item_height < top_y || y_offset > bottom_y {
                y_offset += item_height;
                continue;
            }
            
            let is_current = matches!(item, ExplorerItem::File(idx) if Some(*idx) == active_fi);
            let is_selected = self.explorer_selected_item == Some(_i);
            
            // Draw item background - folder headers get darker, files get highlight when selected
            let item_bg_color = match item {
                ExplorerItem::Folder(_) => (8, 8, 12), // Dark blue-tint for folder headers
                _ if is_selected => (42, 42, 54),
                ExplorerItem::File(_) if is_current => (30, 30, 30),
                _ => (15, 15, 15),
            };
            
            let row_y0 = y_offset as i32;
            let row_y1 = row_y0 + item_height as i32;
            fill_rect_buffer(
                buffer,
                cw,
                ch,
                clip_x0,
                row_y0.max(top_y as i32),
                clip_x1,
                row_y1.min(bottom_y as i32),
                (item_bg_color.0, item_bg_color.1, item_bg_color.2, 0xff),
            );
            
            // Draw text using TextRasterizer
            let text_color = match item {
                ExplorerItem::Folder(_) => (140, 160, 200), // Muted blue for folder names
                ExplorerItem::File(_) if is_current => (0, 255, 0),
                _ => (220, 220, 220),
            };
            let text_y_offset = y_offset + (item_height - rasterizer.font_size) / 2.0;
            let pad_x = clip_x0 as f32 + padding;

            for character in &rasterizer.characters {
                let char_x = pad_x + character.x;
                let char_y = text_y_offset + character.y;
                
                let width = character.width as usize;
                if width == 0 {
                    continue;
                }
                for (bitmap_y, row) in character.bitmap.chunks(width).enumerate() {
                    for (bitmap_x, &alpha) in row.iter().enumerate() {
                        if alpha == 0 {
                            continue;
                        }
                        
                        let px = (char_x + bitmap_x as f32) as i32;
                        let py = (char_y + bitmap_y as f32) as i32;
                        
                        if px >= clip_x0 && px < clip_x1 && py >= top_y as i32 && py < bottom_y as i32 {
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
            if separator_y >= top_y as i32 && separator_y < bottom_y as i32 {
                fill_rect_buffer(
                    buffer,
                    cw,
                    ch,
                    clip_x0,
                    separator_y,
                    clip_x1,
                    separator_y + 1,
                    (30, 30, 30, 0xff),
                );
            }
            
            y_offset += item_height;
        }
    }

    fn draw_dim_overlay(
        buffer: &mut [u8],
        canvas_width: u32,
        canvas_height: u32,
        y0: i32,
        y1: i32,
    ) {
        let fh = canvas_height as i32;
        let y0 = y0.max(0);
        let y1 = y1.min(fh);
        let dim = 0.52_f32;
        let inv = 1.0 - dim;
        for y in y0..y1 {
            let row = (y as u32 * canvas_width) as usize * 4;
            for x in 0..canvas_width as usize {
                let i = row + x * 4;
                if i + 3 >= buffer.len() {
                    break;
                }
                buffer[i] = (buffer[i] as f32 * inv) as u8;
                buffer[i + 1] = (buffer[i + 1] as f32 * inv) as u8;
                buffer[i + 2] = (buffer[i + 2] as f32 * inv) as u8;
            }
        }
    }

    fn draw_editor_tab_bar(
        &self,
        buffer: &mut [u8],
        canvas_width: u32,
        canvas_height: u32,
        safe_top_y: f32,
        bar_h: f32,
        padding: i32,
        ui_scale: f32,
    ) {
        let cw = canvas_width as usize;
        let ch = canvas_height as usize;
        let y0 = safe_top_y as i32;
        let y1 = (safe_top_y + bar_h) as i32;
        fill_rect_buffer(
            buffer,
            cw,
            ch,
            0,
            y0,
            canvas_width as i32,
            y1,
            (10, 10, 12, 0xff),
        );
        let n = self.open_editor_tabs.len().max(1);
        let inner_w = canvas_width as f32 - 2.0 * padding as f32;
        let tab_w = inner_w / n as f32;
        let close_reserve = (30.0_f32 * CODER_CHROME_SCALE * ui_scale.max(0.4)).max(22.0);
        for (i, _tab) in self.open_editor_tabs.iter().enumerate() {
            let x0 = (padding as f32 + i as f32 * tab_w) as i32;
            let x1 = (padding as f32 + (i + 1) as f32 * tab_w) as i32;
            let active = i == self.active_editor_tab;
            let bg = if active {
                (26, 27, 32)
            } else {
                (16, 17, 21)
            };
            fill_rect_buffer(buffer, cw, ch, x0, y0, x1, y1, (bg.0, bg.1, bg.2, 0xff));
            fill_rect_buffer(buffer, cw, ch, x1 - 1, y0, x1, y1, (8, 8, 10, 0xff));

            if i >= self.editor_tab_labels.len() {
                continue;
            }
            let label = &self.editor_tab_labels[i];
            let text_color = if active {
                (235, 236, 240)
            } else {
                (170, 172, 180)
            };
            let path_left = x0 as f32 + 6.0;
            let path_right = x1 as f32 - close_reserve;
            let text_top = safe_top_y + (bar_h - label.font_size) * 0.5;
            for character in &label.characters {
                let char_x = path_left + character.x;
                if char_x + character.metrics.advance_width > path_right {
                    break;
                }
                let char_y = text_top + character.y;
                let wch = character.width as usize;
                if wch == 0 {
                    continue;
                }
                for (bitmap_y, row) in character.bitmap.chunks(wch).enumerate() {
                    for (bitmap_x, &alpha) in row.iter().enumerate() {
                        if alpha == 0 {
                            continue;
                        }
                        let px = (char_x + bitmap_x as f32) as i32;
                        let py = (char_y + bitmap_y as f32) as i32;
                        if px >= x0 && px < x1 && py >= y0 && py < y1 {
                            let idx = ((py as u32 * canvas_width + px as u32) * 4) as usize;
                            if idx + 2 < buffer.len() {
                                let a = alpha as f32 / 255.0;
                                buffer[idx] =
                                    ((text_color.0 as f32 * a) + (buffer[idx] as f32 * (1.0 - a))) as u8;
                                buffer[idx + 1] =
                                    ((text_color.1 as f32 * a) + (buffer[idx + 1] as f32 * (1.0 - a))) as u8;
                                buffer[idx + 2] =
                                    ((text_color.2 as f32 * a) + (buffer[idx + 2] as f32 * (1.0 - a))) as u8;
                            }
                        }
                    }
                }
            }

            let cx0 = (x1 as f32 - close_reserve + 2.0) as i32;
            let cx1 = x1 - 2;
            let tw = self.editor_tab_close_label.characters.iter().map(|c| c.metrics.advance_width).sum::<f32>();
            let tox = ((cx0 + cx1) as f32 * 0.5 - tw * 0.5).max(cx0 as f32);
            let toy = safe_top_y + (bar_h - self.editor_tab_close_label.font_size) * 0.5;
            for character in &self.editor_tab_close_label.characters {
                let char_x = tox + character.x;
                let char_y = toy + character.y;
                let wch = character.width as usize;
                if wch == 0 {
                    continue;
                }
                let tc = (200, 90, 90);
                for (bitmap_y, row) in character.bitmap.chunks(wch).enumerate() {
                    for (bitmap_x, &alpha) in row.iter().enumerate() {
                        if alpha == 0 {
                            continue;
                        }
                        let px = (char_x + bitmap_x as f32) as i32;
                        let py = (char_y + bitmap_y as f32) as i32;
                        if px >= x0 && px < x1 && py >= y0 && py < y1 {
                            let idx = ((py as u32 * canvas_width + px as u32) * 4) as usize;
                            if idx + 2 < buffer.len() {
                                let a = alpha as f32 / 255.0;
                                buffer[idx] =
                                    ((tc.0 as f32 * a) + (buffer[idx] as f32 * (1.0 - a))) as u8;
                                buffer[idx + 1] =
                                    ((tc.1 as f32 * a) + (buffer[idx + 1] as f32 * (1.0 - a))) as u8;
                                buffer[idx + 2] =
                                    ((tc.2 as f32 * a) + (buffer[idx + 2] as f32 * (1.0 - a))) as u8;
                            }
                        }
                    }
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
            (
                (color.0 as u16 * 120 / 100).min(255) as u8,
                (color.1 as u16 * 120 / 100).min(255) as u8,
                (color.2 as u16 * 120 / 100).min(255) as u8,
            )
        } else {
            color
        };
        let cw = canvas_width as usize;
        let ch = canvas_height as usize;
        let x1 = x + width as i32;
        let y1 = y + height as i32;
        fill_rect_buffer(buffer, cw, ch, x, y, x1, y1, (bg_color.0, bg_color.1, bg_color.2, 0xff));
        let border_color = (255, 255, 255);
        let bc = (border_color.0, border_color.1, border_color.2, 0xff);
        fill_rect_buffer(buffer, cw, ch, x, y, x1, y + 1, bc);
        let bottom_y = y1 - 1;
        fill_rect_buffer(buffer, cw, ch, x, bottom_y, x1, y1, bc);
        fill_rect_buffer(buffer, cw, ch, x, y, x + 1, y1, bc);
        fill_rect_buffer(buffer, cw, ch, x1 - 1, y, x1, y1, bc);
        
        // Note: Button text rendering is not implemented yet
        // The button will just show as a colored rectangle
    }

    fn draw_run_button_label(&self, buffer: &mut [u8], canvas_width: u32, canvas_height: u32) {
        let text_color = (255, 255, 255);
        let text_width = self
            .run_button_label
            .characters
            .iter()
            .map(|c| c.metrics.advance_width)
            .sum::<f32>();
        let text_offset_x = (self.run_button.width as f32 - text_width).max(0.0) / 2.0;
        let text_offset_y =
            (self.run_button.height as f32 - self.run_button_label.font_size) / 2.0;
        for character in &self.run_button_label.characters {
            let char_x = self.run_button.x as f32 + character.x + text_offset_x;
            let char_y = self.run_button.y as f32 + character.y + text_offset_y;
            let cw = character.width as usize;
            if cw == 0 {
                continue;
            }
            for (bitmap_y, row) in character.bitmap.chunks(cw).enumerate() {
                for (bitmap_x, &alpha) in row.iter().enumerate() {
                    if alpha == 0 {
                        continue;
                    }
                    let px = (char_x + bitmap_x as f32) as i32;
                    let py = (char_y + bitmap_y as f32) as i32;
                    if px >= 0
                        && px < canvas_width as i32
                        && py >= 0
                        && py < canvas_height as i32
                    {
                        let idx = ((py as u32 * canvas_width + px as u32) * 4) as usize;
                        let alpha_f = alpha as f32 / 255.0;
                        buffer[idx + 0] = ((text_color.0 as f32 * alpha_f)
                            + (buffer[idx + 0] as f32 * (1.0 - alpha_f)))
                            as u8;
                        buffer[idx + 1] = ((text_color.1 as f32 * alpha_f)
                            + (buffer[idx + 1] as f32 * (1.0 - alpha_f)))
                            as u8;
                        buffer[idx + 2] = ((text_color.2 as f32 * alpha_f)
                            + (buffer[idx + 2] as f32 * (1.0 - alpha_f)))
                            as u8;
                    }
                }
            }
        }
    }

    /// Frosted gray strip between content and keyboard; alpha-blends with pixels already in the buffer.
    fn draw_glassy_task_bar(
        buffer: &mut [u8],
        canvas_width: u32,
        canvas_height: u32,
        task_bar_top: f32,
        task_bar_bottom: f32,
    ) {
        let y0 = task_bar_top.round().clamp(0.0, canvas_height as f32 - 1.0) as i32;
        let y1 = task_bar_bottom.round().clamp(y0 as f32 + 1.0, canvas_height as f32) as i32;
        if y0 >= y1 {
            return;
        }
        // Dark frosted strip — slightly above pitch black
        let (r, g, b, a) = (22u8, 22u8, 24u8, 245u8);
        let af = a as f32 / 255.0;
        for y in y0..y1 {
            let row = (y as u32 * canvas_width) as usize * 4;
            for x in 0..canvas_width as usize {
                let i = row + x * 4;
                if i + 3 >= buffer.len() {
                    break;
                }
                let inv = 1.0 - af;
                buffer[i + 0] = (r as f32 * af + buffer[i + 0] as f32 * inv) as u8;
                buffer[i + 1] = (g as f32 * af + buffer[i + 1] as f32 * inv) as u8;
                buffer[i + 2] = (b as f32 * af + buffer[i + 2] as f32 * inv) as u8;
                buffer[i + 3] = 0xff;
            }
        }
        // Thin top highlight
        let hl = (48u8, 48u8, 52u8, 90u8);
        let haf = hl.3 as f32 / 255.0;
        let yh = y0.max(0);
        if yh < canvas_height as i32 {
            let row = (yh as u32 * canvas_width) as usize * 4;
            for x in 0..canvas_width as usize {
                let i = row + x * 4;
                if i + 3 >= buffer.len() {
                    break;
                }
                let inv = 1.0 - haf;
                buffer[i + 0] = (hl.0 as f32 * haf + buffer[i + 0] as f32 * inv) as u8;
                buffer[i + 1] = (hl.1 as f32 * haf + buffer[i + 1] as f32 * inv) as u8;
                buffer[i + 2] = (hl.2 as f32 * haf + buffer[i + 2] as f32 * inv) as u8;
            }
        }
    }
}

impl Drop for CoderApp {
    fn drop(&mut self) {
        // Clean up all audio resources (microphones and speakers) when CoderApp is dropped
        // This ensures audio devices are stopped when switching apps
        crate::python_api::audio::cleanup_all_audio();
        
        // Disable coder logging
        super::logging::disable_coder_logging();
    }
}

impl Application for CoderApp {
    fn setup(&mut self, state: &mut EngineState) -> Result<(), String> {
        // Setup all text apps
        self.code_app.setup(state)?;
        self.terminal_app.setup(state)?;
        self.console_app.setup(state)?;
        
        // Override code_app scroll to start at top (TextApp's setup sets it to 1/3 down on iOS)
        self.code_app.scroll_y = 0.0;
        self.code_app.scroll_target = 0.0;
        self.code_app.clear_wheel_scroll_accel();
        
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        // Update terminal from background thread output
        if let Ok(buffer) = self.python_output_buffer.try_lock() {
            if !buffer.is_empty() {
                let s = buffer.as_str();
                if s != self.terminal_app.text_rasterizer.text.as_str() {
                    self.terminal_app.text_rasterizer.text = buffer.clone();
                }
            }
        }
        
        // Disabled: Rust/Swift logs are too noisy and interfere with Python output
        // Only show Python output in the terminal for now
        // if super::logging::has_pending_logs() {
        //     let logs = super::logging::read_pending_logs();
        //     if !logs.is_empty() {
        //         let is_background_running = self.python_thread_running.lock().map(|f| *f).unwrap_or(false);
        //         if !is_background_running {
        //             self.terminal_app.text_rasterizer.text.push_str(&logs);
        //         }
        //     }
        // }
        
        // Check if background thread is done and clean it up
        if let Some(handle) = &self.python_thread_handle {
            if handle.is_finished() {
                if let Some(handle) = self.python_thread_handle.take() {
                    let _ = handle.join();
                }
            }
        }
        
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
        let shape = state.frame.tensor.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        let mouse_x = state.mouse.x;
        let mouse_y = state.mouse.y;

        let ui_scale = Self::layout_scale_from_state(state);
        self.apply_coder_ui_scale(ui_scale);

        // Get keyboard top edge coordinates (normalized 0-1)
        let (_, keyboard_top_y, _, _) = state.keyboard.onscreen.top_edge_coordinates();
        let keyboard_top_px = keyboard_top_y * height;

        let (_button_w, button_height) = Self::button_size_scaled(ui_scale);
        let padding = Self::padding_scaled(ui_scale);
        
        // Bottom task bar (~6.5% height at 1.3×, at least tall enough for tab buttons); moves up with the keyboard.
        let task_bar_height = (height * 0.05_f32 * CODER_CHROME_SCALE)
            .max(button_height as f32 + padding as f32);
        self.code_app.bottom_chrome_height_px = task_bar_height;
        self.terminal_app.bottom_chrome_height_px = task_bar_height;
        
        let task_bar_top = keyboard_top_px - task_bar_height;
        let task_bar_bottom = keyboard_top_px;

        // One shared height for tabs, run/stop/clear, and console input strip (fills task bar vertically).
        let chrome_h = (task_bar_height - 8.0_f32 * CODER_CHROME_SCALE)
            .max(button_height as f32)
            .round() as u32;
        self.run_button.height = chrome_h;
        self.stop_button.height = chrome_h;
        self.stop_button.width = chrome_h;
        self.clear_button.height = chrome_h;
        self.clear_button.width = chrome_h;
        
        // Console is only shown on terminal tab
        let show_console = self.active_tab == Tab::Terminal;
        let console_height = chrome_h;
        
        // Console sits above the task bar when on terminal tab
        let console_bottom_y = if show_console {
            task_bar_top - padding as f32
        } else {
            task_bar_bottom
        };
        let console_top_y = console_bottom_y - console_height as f32;
        
        // Tabs/buttons are vertically centered inside the task bar
        let tab_top_y =
            (task_bar_top + (task_bar_height - chrome_h as f32) * 0.5).round() as i32;

        self.code_app.top_chrome_height_px = if self.active_tab == Tab::Code {
            Self::editor_tab_bar_height_px(ui_scale)
        } else {
            0.0
        };
        
        // Delegate to active text app (but not console - it gets special handling)
        match self.active_tab {
            Tab::Code => {
                self.code_app.tick(state);
            }
            Tab::Terminal => self.terminal_app.tick(state),
            Tab::Viewport => {
                // Render Python app if available, otherwise show black screen
                if let Some(ref app_instance) = self.viewport_app {
                    fill(&mut state.frame, (0, 0, 0, 0xff));
                    
                    // Setup Python app if not done yet
                    if !self.viewport_app_setup_done {
                        let setup_result = self.interpreter.enter(|vm| {
                            // Register Application class and _FrameWrapper in builtins
                            let app_class_code = crate::python_api::engine::pyapp::APPLICATION_CLASS_CODE;
                            let scope = vm.new_scope_with_builtins();
                            if let Err(e) = vm.run_code_string(scope, app_class_code, "<viewport_setup>".to_string()) {
                                eprintln!("Failed to register Application class: {:?}", e);
                                return Err(());
                            }
                            
                            // Create Python frame object from engine state
                            let frame_dict = crate::python_api::engine::py_bindings::create_py_frame_state(vm, &mut state.frame)
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
                                let error_text = crate::python_api::runtime::format_python_exception(vm, &e);
                                
                                // Write error to output buffer
                                if let Ok(mut buffer) = self.python_output_buffer.lock() {
                                    buffer.push_str("\nPython setup error:\n");
                                    buffer.push_str(&error_text);
                                    buffer.push_str("\n");
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
                        let shape = state.frame.tensor.shape();
                        let width = shape[1];
                        let height = shape[0];
                        let buffer = state.frame.buffer_mut();
                        crate::python_api::rasterizer::set_frame_buffer_context(buffer, width, height);
                        
                        self.interpreter.enter(|vm| {
                            // Update frame data before calling tick
                            if let Ok(Some(frame_obj)) = vm.get_attribute_opt(app_instance.clone(), "frame") {
                                let _ = crate::python_api::engine::py_bindings::update_py_frame_state(vm, frame_obj.clone(), &mut state.frame);
                                
                                // Update mouse data
                                let mouse_dict = vm.ctx.new_dict();
                                let _ = mouse_dict.set_item("x", vm.ctx.new_float(state.mouse.x as f64).into(), vm);
                                let _ = mouse_dict.set_item("y", vm.ctx.new_float(state.mouse.y as f64).into(), vm);
                                let _ = mouse_dict.set_item("is_left_clicking", vm.ctx.new_bool(state.mouse.is_left_clicking).into(), vm);
                                let _ = app_instance.set_attr("mouse", mouse_dict, vm);
                                
                                // Call tick
                                if let Err(e) = vm.call_method(app_instance, "tick", ()) {
                                    let error_text = crate::python_api::runtime::format_python_exception(vm, &e);
                                    
                                    // Write error to output buffer
                                    if let Ok(mut buffer) = self.python_output_buffer.lock() {
                                        buffer.push_str("\nPython tick error:\n");
                                        buffer.push_str(&error_text);
                                        buffer.push_str("\n");
                                    }
                                }
                            }
                        });
                        
                        // Clear the frame buffer context after tick
                        crate::python_api::rasterizer::clear_frame_buffer_context();
                    }
                } else {
                    fill(&mut state.frame, (0, 0, 0, 0xff));
                }
            }
        }
        
        let keyboard_is_shown = state.keyboard.onscreen.is_shown();

        if keyboard_is_shown {
            self.console_app
                .text_rasterizer
                .tick(width, console_height as f32);
            self.clear_button_label.tick(width, height);
        }
        // Task bar labels tick every frame (task bar is always visible).
        self.code_tab_label.tick(width, height);
        self.terminal_tab_label.tick(width, height);
        self.viewport_tab_label.tick(width, height);

        if let Some(afi) = self.active_file_index() {
            if afi < self.python_files.len() {
                let fname = Self::file_basename(&self.python_files[afi].name);
                if self.run_button_label.text != fname {
                    self.run_button_label.set_text(fname);
                }
            }
        } else if !self.run_button_label.text.is_empty() {
            self.run_button_label.set_text(String::new());
        }
        self.run_button_label.tick(width, height);
        let (bw_base, _) = Self::button_size_scaled(ui_scale);
        let text_w: f32 = self
            .run_button_label
            .characters
            .iter()
            .map(|c| c.metrics.advance_width)
            .sum();
        self.run_button.width = ((text_w + 36.0)
            .max(bw_base as f32)
            .min(width * 0.55_f32)) as u32;

        if self.active_tab == Tab::Code && self.explorer_popup_open {
            for rasterizer in &mut self.file_list_rasterizers {
                rasterizer.tick(width, height);
            }
        }
        if self.active_tab == Tab::Code {
            for (i, tab) in self.open_editor_tabs.iter().enumerate() {
                if i < self.editor_tab_labels.len() {
                    let path = self.python_files[tab.file_index].name.clone();
                    if self.editor_tab_labels[i].text != path {
                        self.editor_tab_labels[i].set_text(path);
                    }
                    self.editor_tab_labels[i].tick(width, height);
                }
            }
            self.editor_tab_close_label.tick(width, height);
        }
        
        // Get safe region top boundary (in pixels) before mutable borrow
        let safe_region_top_y = state.frame.safe_region_boundaries.y1 * height;
        
        // Get buffer again for drawing console, file explorer, tabs and buttons on top
        let buffer = state.frame_buffer_mut();
        
        if self.active_tab == Tab::Code {
            let bar_h = Self::editor_tab_bar_height_px(ui_scale);
            self.draw_editor_tab_bar(
                buffer,
                width as u32,
                height as u32,
                safe_region_top_y,
                bar_h,
                padding,
                ui_scale,
            );
            if self.explorer_popup_open {
                let (px, pt, px1, pb) = Self::explorer_panel_bounds(
                    width,
                    height,
                    safe_region_top_y,
                    bar_h,
                    task_bar_top,
                    ui_scale,
                );
                Self::draw_dim_overlay(
                    buffer,
                    width as u32,
                    height as u32,
                    (safe_region_top_y + bar_h) as i32,
                    task_bar_top as i32,
                );
                let clip_x0 = px.round() as i32;
                let clip_x1 = px1.round() as i32;
                fill_rect_buffer(
                    buffer,
                    width as usize,
                    height as usize,
                    clip_x0,
                    pt as i32,
                    clip_x1,
                    pb as i32,
                    (22, 22, 28, 0xff),
                );
                fill_rect_buffer(
                    buffer,
                    width as usize,
                    height as usize,
                    clip_x0,
                    pt as i32,
                    clip_x1,
                    pt as i32 + 1,
                    (80, 85, 95, 0xff),
                );
                self.draw_file_explorer(
                    buffer,
                    width as u32,
                    height as u32,
                    pt,
                    pb,
                    clip_x0,
                    clip_x1,
                    ui_scale,
                );
            }
        }
        
        // Glassy task bar strip (always — moves up when the keyboard opens)
        Self::draw_glassy_task_bar(
            buffer,
            width as u32,
            height as u32,
            task_bar_top,
            task_bar_bottom,
        );
        
        // Console above task bar (only when keyboard is up and on terminal tab)
        if keyboard_is_shown && show_console {
            let fw = width as usize;
            let fh = height as usize;
            let w_i = width as i32;
            fill_rect_buffer(
                buffer,
                fw,
                fh,
                0,
                console_top_y as i32,
                w_i,
                console_bottom_y as i32,
                (20, 20, 20, 0xff),
            );
            
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
            
            fill_rect_buffer(
                buffer,
                fw,
                fh,
                0,
                console_top_y as i32,
                w_i,
                console_top_y as i32 + 1,
                (100, 100, 100, 0xff),
            );
        }
        
        let tab_width = Self::tab_width_scaled(ui_scale);
        let tab_height = chrome_h;
        
        // Draw files tab on the left
        self.draw_tab(buffer, width as u32, height as u32, padding, tab_top_y, tab_width, tab_height, &self.code_tab_label, self.active_tab == Tab::Code);
        
        // Viewport in the middle, terminal on the right
        self.draw_tab(
            buffer,
            width as u32,
            height as u32,
            padding + tab_width as i32,
            tab_top_y,
            tab_width,
            tab_height,
            &self.viewport_tab_label,
            self.active_tab == Tab::Viewport,
        );
        self.draw_tab(
            buffer,
            width as u32,
            height as u32,
            padding + (tab_width * 2) as i32,
            tab_top_y,
            tab_width,
            tab_height,
            &self.terminal_tab_label,
            self.active_tab == Tab::Terminal,
        );
        
        // Check if viewport app or background thread is running
        let is_viewport_app_running = self.viewport_app.is_some();
        let is_background_thread_running = self.python_thread_running.lock().map(|f| *f).unwrap_or(false);
        let is_any_app_running = is_viewport_app_running || is_background_thread_running;
        
        // Position buttons on the right side
        // If app is running, show stop button to the left of run button
        if is_any_app_running {
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
        let should_execute_console = keyboard_is_shown && show_console && console_has_text;
        
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
        self.draw_run_button_label(buffer, width as u32, height as u32);

        // Position and draw clear "×" (only when console is shown and has text)
        if keyboard_is_shown && show_console && console_has_text {
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
                
                let cw = character.width as usize;
                if cw == 0 { continue; }
                for (bitmap_y, row) in character.bitmap.chunks(cw).enumerate() {
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
                if self.explorer_popup_open {
                    self.file_list_scroll_y += dy;
                    let scale = Self::layout_scale_from_state(state);
                    let max_scroll = self.explorer_items.iter()
                        .filter(|i| self.is_item_visible(i))
                        .map(|i| CoderApp::explorer_item_height(i, scale))
                        .sum::<f32>()
                        .max(0.0);
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
                if self.explorer_popup_open {
                    match ch {
                        '\u{2191}' => self.explorer_move_selection(-1), // ↑
                        '\u{2193}' => self.explorer_move_selection(1),  // ↓
                        '\n' | '\r' => self.explorer_activate_selected(),
                        _ => {}
                    }
                    return;
                }
                self.code_app.on_key_char(state, ch);
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
                if self.explorer_popup_open {
                    if !self.file_explorer_dragging && state.mouse.is_left_clicking {
                        let dx = (state.mouse.x - self.file_explorer_last_tap_x).abs();
                        let dy = (state.mouse.y - self.file_explorer_last_tap_y).abs();
                        if dx > 5.0 || dy > 5.0 {
                            self.file_explorer_dragging = true;
                            self.file_explorer_last_mouse_y = state.mouse.y;
                        }
                    }
                    if self.file_explorer_dragging {
                        let dy = state.mouse.y - self.file_explorer_last_mouse_y;
                        self.file_list_scroll_y -= dy;
                        let scale = Self::layout_scale_from_state(state);
                        let max_scroll = self.explorer_items.iter()
                            .filter(|i| self.is_item_visible(i))
                            .map(|i| CoderApp::explorer_item_height(i, scale))
                            .sum::<f32>()
                            .max(0.0);
                        self.file_list_scroll_y = self.file_list_scroll_y.max(0.0).min(max_scroll);
                        self.file_explorer_last_mouse_y = state.mouse.y;
                    }
                } else {
                    self.code_app.on_mouse_move(state);
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
        
        let keyboard_is_shown = state.keyboard.onscreen.is_shown();
        
        let layout_scale = Self::layout_scale_from_state(state);
        let (_, button_height) = Self::button_size_scaled(layout_scale);
        let tab_width = Self::tab_width_scaled(layout_scale);
        let padding = Self::padding_scaled(layout_scale);
        
        let shape = state.frame.tensor.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        let safe_top_y = state.frame.safe_region_boundaries.y1 * height;
        let (_, keyboard_top_y, _, _) = state.keyboard.onscreen.top_edge_coordinates();
        let keyboard_top_px = keyboard_top_y * height;
        
        let task_bar_height = (height * 0.05_f32 * CODER_CHROME_SCALE)
            .max(button_height as f32 + padding as f32);
        let task_bar_top = keyboard_top_px - task_bar_height;
        let chrome_h = (task_bar_height - 8.0_f32 * CODER_CHROME_SCALE)
            .max(button_height as f32)
            .round() as u32;
        let tab_top_y = (task_bar_top + (task_bar_height - chrome_h as f32) * 0.5).round() as i32;

        if self.active_tab == Tab::Code {
            let bar_h = Self::editor_tab_bar_height_px(layout_scale);
            if let Some(h) = self.editor_tab_bar_mouse_hit(
                mouse_x,
                mouse_y,
                safe_top_y,
                bar_h,
                padding,
                width,
                layout_scale,
            ) {
                self.sync_editor_buffer_to_active_tab();
                match h {
                    EditorBarHit::Close(i) => {
                        self.close_editor_tab_at(i);
                    }
                    EditorBarHit::Select(i) => {
                        self.switch_editor_tab_to(i);
                    }
                }
                return;
            }
            if self.explorer_popup_open {
                let (px, pt, px1, pb) = Self::explorer_panel_bounds(
                    width,
                    height,
                    safe_top_y,
                    bar_h,
                    task_bar_top,
                    layout_scale,
                );
                let in_panel =
                    mouse_x >= px && mouse_x <= px1 && mouse_y >= pt && mouse_y <= pb;
                let in_backdrop = mouse_y >= safe_top_y + bar_h && mouse_y < task_bar_top;
                if in_backdrop && !in_panel {
                    self.explorer_popup_open = false;
                    return;
                }
                if in_panel {
                    self.file_explorer_last_tap_x = mouse_x;
                    self.file_explorer_last_tap_y = mouse_y;
                    self.file_explorer_dragging = false;
                    return;
                }
            }
        }
        
        // Task bar chrome (tabs / run / stop) — always hit-test; matches visible task bar
        {
            if self.tab_contains_point(mouse_x, mouse_y, padding, tab_top_y, tab_width, chrome_h) {
                if self.active_tab == Tab::Code {
                    self.save_current_file();
                    self.explorer_popup_open = !self.explorer_popup_open;
                } else {
                    self.active_tab = Tab::Code;
                }
                return;
            }
            
            // Check if click is on viewport tab (middle)
            if self.tab_contains_point(
                mouse_x,
                mouse_y,
                padding + tab_width as i32,
                tab_top_y,
                tab_width,
                chrome_h,
            ) {
                self.active_tab = Tab::Viewport;
                return;
            }
            
            // Check if click is on terminal tab (right)
            if self.tab_contains_point(
                mouse_x,
                mouse_y,
                padding + (tab_width * 2) as i32,
                tab_top_y,
                tab_width,
                chrome_h,
            ) {
                self.active_tab = Tab::Terminal;
                return;
            }
            
            // Clear button only when keyboard is up and console strip is visible
            let console_has_text = !self.console_app.text_rasterizer.text.trim().is_empty();
            if keyboard_is_shown && self.active_tab == Tab::Terminal && console_has_text {
                if self.clear_button.contains_point(mouse_x, mouse_y) {
                    // Clear the console input
                    self.console_app.text_rasterizer.text.clear();
                    self.console_app.cursor_position = 0;
                    return;
                }
            }
            
            // Check if click is on the stop button (only visible when app/thread is running)
            let is_viewport_running = self.viewport_app.is_some();
            let is_background_running = self.python_thread_running.lock().map(|f| *f).unwrap_or(false);
            
            if (is_viewport_running || is_background_running) && self.stop_button.contains_point(mouse_x, mouse_y) {
                // Stop viewport app if running
                if is_viewport_running {
                    self.viewport_app = None;
                    self.viewport_app_setup_done = false;
                    // Clean up all audio resources immediately
                    crate::python_api::audio::cleanup_all_audio();
                    self.terminal_app.text_rasterizer.text.push_str("\n[xos] Viewport app stopped\n");
                }
                
                // Stop background thread if running
                if is_background_running {
                    // Increment generation counter - this orphans the old thread's output
                    if let Ok(mut gen) = self.python_thread_generation.lock() {
                        *gen += 1;
                    }
                    
                    // Mark as not running
                    if let Ok(mut flag) = self.python_thread_running.lock() {
                        *flag = false;
                    }
                    
                    // Drop the thread handle - let it finish in background
                    self.python_thread_handle = None;
                    
                    // Clean up all audio resources immediately (CRITICAL FIX!)
                    crate::python_api::audio::cleanup_all_audio();
                    
                    // Update terminal with final message
                    if let Ok(mut buffer) = self.python_output_buffer.lock() {
                        buffer.push_str("\n[xos] Script stopped by user\n");
                    }
                }
                
                return;
            }
            
            // Check if click is on the run button
            if self.run_button.contains_point(mouse_x, mouse_y) {
                // Determine if we should execute console command or run code
                let show_console = self.active_tab == Tab::Terminal;
                let console_has_text = !self.console_app.text_rasterizer.text.trim().is_empty();
                
                if keyboard_is_shown && show_console && console_has_text {
                    // Execute console command and clear it
                    let command = self.console_app.text_rasterizer.text.clone();
                    self.execute_console_command(&command);
                    // Clear the console input after execution
                    self.console_app.text_rasterizer.text.clear();
                    self.console_app.cursor_position = 0;
                } else {
                    // Execute the Python code from code tab
                    let code = self.code_app.text_rasterizer.text.clone();
                    if !code.trim().is_empty() {
                        self.execute_python_code(&code);
                    }
                }
                return;
            }
        }
        
        match self.active_tab {
            Tab::Code => {
                if !self.explorer_popup_open {
                    self.code_app.on_mouse_down(state);
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
                if self.explorer_popup_open {
                    let mouse_x = state.mouse.x;
                    let mouse_y = state.mouse.y;
                    let dx = (mouse_x - self.file_explorer_last_tap_x).abs();
                    let dy = (mouse_y - self.file_explorer_last_tap_y).abs();
                    let drag_threshold = 10.0_f32;
                    if !self.file_explorer_dragging && dx < drag_threshold && dy < drag_threshold {
                        let shape = state.frame.tensor.shape();
                        let height = shape[0] as f32;
                        let width = shape[1] as f32;
                        let safe_region_top_y = state.frame.safe_region_boundaries.y1 * height;
                        let (_, keyboard_top_y, _, _) = state.keyboard.onscreen.top_edge_coordinates();
                        let keyboard_top_px = keyboard_top_y * height;
                        let layout_scale = Self::layout_scale_from_state(state);
                        let padding = Self::padding_scaled(layout_scale);
                        let (_, button_height) = Self::button_size_scaled(layout_scale);
                        let task_bar_height = (height * 0.05_f32 * CODER_CHROME_SCALE)
                            .max(button_height as f32 + padding as f32);
                        let task_bar_top = keyboard_top_px - task_bar_height;
                        let bar_h = Self::editor_tab_bar_height_px(layout_scale);
                        let (px, pt, px1, pb) = Self::explorer_panel_bounds(
                            width,
                            height,
                            safe_region_top_y,
                            bar_h,
                            task_bar_top,
                            layout_scale,
                        );
                        if mouse_x >= px && mouse_x <= px1 && mouse_y >= pt && mouse_y <= pb {
                            let click_y_in_list = mouse_y - pt + self.file_list_scroll_y;
                            let mut y = 0.0f32;
                            for (idx, item) in self.explorer_items.iter().enumerate() {
                                if !self.is_item_visible(item) {
                                    continue;
                                }
                                let item_height = CoderApp::explorer_item_height(item, layout_scale);
                                if click_y_in_list >= y && click_y_in_list < y + item_height {
                                    match item {
                                        ExplorerItem::Folder(name) => {
                                            if self.expanded_folders.contains(name) {
                                                self.expanded_folders.remove(name);
                                                self.file_list_rasterizers[idx]
                                                    .set_text(format!("  ▶  {}/", name));
                                            } else {
                                                self.expanded_folders.insert(name.clone());
                                                self.file_list_rasterizers[idx]
                                                    .set_text(format!("  ▼  {}/", name));
                                            }
                                        }
                                        ExplorerItem::File(file_index) => {
                                            self.open_or_select_file_from_explorer(*file_index);
                                        }
                                    }
                                    break;
                                }
                                y += item_height;
                            }
                        }
                    }
                    self.file_explorer_dragging = false;
                } else {
                    self.code_app.on_mouse_up(state);
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

    fn on_key_shortcut(&mut self, state: &mut EngineState, shortcut: ShortcutAction) {
        match shortcut {
            ShortcutAction::Tab1 => {
                self.active_tab = Tab::Code;
                self.explorer_popup_open = false;
                self.switch_editor_tab_to(0);
            }
            ShortcutAction::Tab2 => {
                self.active_tab = Tab::Code;
                self.explorer_popup_open = false;
                self.switch_editor_tab_to(1);
            }
            ShortcutAction::Tab3 => {
                self.active_tab = Tab::Code;
                self.explorer_popup_open = false;
                self.switch_editor_tab_to(2);
            }
            ShortcutAction::CloseTab => {
                if self.active_tab == Tab::Code {
                    self.close_editor_tab_at(self.active_editor_tab);
                }
            }
            ShortcutAction::ReopenClosedTab => {
                if self.active_tab == Tab::Code {
                    self.reopen_last_closed_editor_file();
                }
            }
            ShortcutAction::ToggleExplorer => {
                if self.active_tab == Tab::Code {
                    self.explorer_popup_open = !self.explorer_popup_open;
                    if self.explorer_popup_open {
                        self.ensure_explorer_selection();
                    }
                }
            }
            ShortcutAction::ShowTerminal => {
                self.active_tab = Tab::Terminal;
            }
            ShortcutAction::ShowViewport => {
                self.active_tab = Tab::Viewport;
            }
            ShortcutAction::Run => {
                self.activate_run_button(state);
            }
            ShortcutAction::TerminateProgram => {
                #[cfg(not(target_arch = "wasm32"))]
                crate::engine::native_engine::request_exit();
            }
            ShortcutAction::PrevModeTab => self.switch_mode_tab_by(-1),
            ShortcutAction::NextModeTab => self.switch_mode_tab_by(1),
            ShortcutAction::PrevEditorTab => {
                self.active_tab = Tab::Code;
                self.explorer_popup_open = false;
                self.switch_editor_tab_by(-1);
            }
            ShortcutAction::NextEditorTab => {
                self.active_tab = Tab::Code;
                self.explorer_popup_open = false;
                self.switch_editor_tab_by(1);
            }
            _ => {}
        }
    }
}

