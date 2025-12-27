//! Core types and enums for the Coder app

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tab {
    Code,
    Terminal,
    Viewport,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CodeViewMode {
    Editor,
    FileExplorer,
}

#[derive(Debug, Clone)]
pub struct PythonFile {
    pub name: String,
    pub content: String,
    #[allow(dead_code)]
    pub path: String,
}

/// Get button/tab dimensions based on platform
pub fn get_button_size() -> (u32, u32) {
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

/// Get tab width (narrower on iOS to make room for stop button)
pub fn get_tab_width() -> u32 {
    let (button_width, _) = get_button_size();
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

