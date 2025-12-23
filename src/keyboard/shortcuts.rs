/// Keyboard shortcuts system for desktop platforms (macOS, Windows, Linux)
/// 
/// Handles common text editing shortcuts like:
/// - Copy: Cmd+C (Mac) / Ctrl+C (Windows/Linux)
/// - Cut: Cmd+X (Mac) / Ctrl+X (Windows/Linux)
/// - Paste: Cmd+V (Mac) / Ctrl+V (Windows/Linux)
/// - Select All: Cmd+A (Mac) / Ctrl+A (Windows/Linux)
/// - Undo: Cmd+Z (Mac) / Ctrl+Z (Windows/Linux)
/// - Redo: Cmd+Shift+Z (Mac) / Ctrl+Y (Windows/Linux)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutAction {
    Copy,
    Cut,
    Paste,
    SelectAll,
    Undo,
    Redo,
}

/// Detects if a keyboard shortcut action was triggered
/// 
/// # Arguments
/// * `ch` - The character that was pressed
/// * `command_held` - Whether Command (macOS) or Ctrl (Windows/Linux) is held
/// * `shift_held` - Whether Shift is held
/// 
/// # Returns
/// `Some(ShortcutAction)` if a shortcut was detected, `None` otherwise
pub fn detect_shortcut(ch: char, command_held: bool, shift_held: bool) -> Option<ShortcutAction> {
    if !command_held {
        return None;
    }
    
    // Normalize character to lowercase for comparison
    let ch_lower = ch.to_lowercase().next().unwrap_or(ch);
    
    match (ch_lower, shift_held) {
        ('c', false) => Some(ShortcutAction::Copy),
        ('x', false) => Some(ShortcutAction::Cut),
        ('v', false) => Some(ShortcutAction::Paste),
        ('a', false) => Some(ShortcutAction::SelectAll),
        ('z', false) => Some(ShortcutAction::Undo),
        ('z', true) => Some(ShortcutAction::Redo),  // Cmd+Shift+Z (Mac)
        ('y', false) => Some(ShortcutAction::Redo), // Ctrl+Y (Windows)
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_copy_shortcut() {
        assert_eq!(detect_shortcut('c', true, false), Some(ShortcutAction::Copy));
        assert_eq!(detect_shortcut('C', true, false), Some(ShortcutAction::Copy));
        assert_eq!(detect_shortcut('c', false, false), None);
    }

    #[test]
    fn test_cut_shortcut() {
        assert_eq!(detect_shortcut('x', true, false), Some(ShortcutAction::Cut));
        assert_eq!(detect_shortcut('X', true, false), Some(ShortcutAction::Cut));
    }

    #[test]
    fn test_paste_shortcut() {
        assert_eq!(detect_shortcut('v', true, false), Some(ShortcutAction::Paste));
        assert_eq!(detect_shortcut('V', true, false), Some(ShortcutAction::Paste));
    }

    #[test]
    fn test_select_all_shortcut() {
        assert_eq!(detect_shortcut('a', true, false), Some(ShortcutAction::SelectAll));
        assert_eq!(detect_shortcut('A', true, false), Some(ShortcutAction::SelectAll));
    }

    #[test]
    fn test_undo_shortcut() {
        assert_eq!(detect_shortcut('z', true, false), Some(ShortcutAction::Undo));
        assert_eq!(detect_shortcut('Z', true, false), Some(ShortcutAction::Undo));
    }

    #[test]
    fn test_redo_shortcuts() {
        // Mac style: Cmd+Shift+Z
        assert_eq!(detect_shortcut('z', true, true), Some(ShortcutAction::Redo));
        // Windows style: Ctrl+Y
        assert_eq!(detect_shortcut('y', true, false), Some(ShortcutAction::Redo));
    }
}


