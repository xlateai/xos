//! Queued on-screen keyboard toggles from Python (e.g. `xos.keyboard.toggle_onscreen`).
//! Consumed once per viewport event (`PyApp::on_mouse_down` after `on_viewport_double_tap`).

use std::sync::atomic::{AtomicU32, Ordering};

static PENDING_OSK_TOGGLES: AtomicU32 = AtomicU32::new(0);

#[inline]
pub fn request_onscreen_keyboard_toggle() {
    PENDING_OSK_TOGGLES.fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn take_pending_osk_toggles() -> u32 {
    PENDING_OSK_TOGGLES.swap(0, Ordering::Acquire)
}

pub fn apply_pending_osk_toggles(keyboard: &mut crate::engine::KeyboardState, count: u32) {
    for _ in 0..count {
        keyboard.onscreen.toggle_minimize();
    }
}
