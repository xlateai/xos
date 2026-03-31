use crate::engine::keyboard::shortcuts::{NamedSpecialKey, PhysicalSpecialKey, SpecialKeyEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoderShortcutAction {
    Tab1,
    Tab2,
    Tab3,
    CloseTab,
    ReopenClosedTab,
    ToggleExplorer,
    FocusExplorerSearch,
    ShowTerminal,
    ShowViewport,
    Run,
    StopExecution,
    TerminateProgram,
    PrevModeTab,
    NextModeTab,
    PrevEditorTab,
    NextEditorTab,
    ToggleViewportTaskbar,
    ToggleBorderlessFullscreen,
}

pub fn detect_coder_shortcut(event: &SpecialKeyEvent) -> Option<CoderShortcutAction> {
    if event.alt_held && !event.shift_held {
        if matches!(event.named_key, Some(NamedSpecialKey::ArrowLeft)) {
            return Some(CoderShortcutAction::PrevEditorTab);
        }
        if matches!(event.named_key, Some(NamedSpecialKey::ArrowRight)) {
            return Some(CoderShortcutAction::NextEditorTab);
        }
    }

    if event.alt_held && event.shift_held {
        if matches!(event.named_key, Some(NamedSpecialKey::ArrowLeft)) {
            return Some(CoderShortcutAction::PrevModeTab);
        }
        if matches!(event.named_key, Some(NamedSpecialKey::ArrowRight)) {
            return Some(CoderShortcutAction::NextModeTab);
        }
    }

    let key = event.physical_key?;

    if event.alt_held && !event.shift_held {
        return match key {
            PhysicalSpecialKey::Digit1 => Some(CoderShortcutAction::Tab1),
            PhysicalSpecialKey::Digit2 => Some(CoderShortcutAction::Tab2),
            PhysicalSpecialKey::Digit3 => Some(CoderShortcutAction::Tab3),
            PhysicalSpecialKey::KeyQ => Some(CoderShortcutAction::CloseTab),
            PhysicalSpecialKey::KeyW => Some(CoderShortcutAction::Run),
            PhysicalSpecialKey::KeyE => Some(CoderShortcutAction::ToggleExplorer),
            PhysicalSpecialKey::KeyF => Some(CoderShortcutAction::ToggleViewportTaskbar),
            PhysicalSpecialKey::KeyT => Some(CoderShortcutAction::ShowTerminal),
            PhysicalSpecialKey::KeyR => Some(CoderShortcutAction::ShowViewport),
        };
    }

    if event.alt_held && event.shift_held {
        return match key {
            PhysicalSpecialKey::KeyE => Some(CoderShortcutAction::FocusExplorerSearch),
            PhysicalSpecialKey::KeyF => Some(CoderShortcutAction::ToggleBorderlessFullscreen),
            PhysicalSpecialKey::KeyW => Some(CoderShortcutAction::StopExecution),
            PhysicalSpecialKey::KeyQ => {
                if event.command_held {
                    Some(CoderShortcutAction::TerminateProgram)
                } else {
                    Some(CoderShortcutAction::ReopenClosedTab)
                }
            }
            _ => None,
        };
    }

    None
}
