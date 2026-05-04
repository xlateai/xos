//! Audio capture toggle + hold-to-open input device menu (same interaction model as `audio_relay`).
//! Renders a square control (green = capture on, gray = off) and an optional single-column overlay.

use crate::engine::audio::{default_input, AudioDevice};
use crate::engine::EngineState;
use crate::rasterizer::text::text_rasterization::TextRasterizer;
use fontdue::Font;
use std::time::{Duration, Instant};

const BUTTON_BORDER_WIDTH: f32 = 3.0;
const HOLD_DURATION: Duration = Duration::from_millis(250);
const MENU_PADDING: f32 = 20.0;
const MENU_ITEM_HEIGHT: f32 = 50.0;
const MENU_COLUMN_WIDTH_RATIO: f32 = 0.45;

/// Result of handling `pointer_up` on the audio input selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioInputSelectorUp {
    None,
    /// Quick tap on the capture button: new enabled state (green on / gray off).
    CaptureToggled(bool),
}

/// Result of handling `pointer_down` while the input menu is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioInputMenuDown {
    /// Click outside the menu column — menu should close.
    Dismiss,
    /// Click on the title bar / padding inside the column — menu should close.
    DismissInColumn,
    /// Picked “Default” or a device row. Recreate the listener if `selection_changed`.
    Pick {
        selection_changed: bool,
    },
}

/// Hold-to-menu + tap-to-toggle for a single capture input list (`devices()` inputs only).
pub struct AudioInputSelector {
    /// When true, capture is intended to be active (green). Default: on.
    pub enabled: bool,
    pub show_menu: bool,
    pub input_devices: Vec<AudioDevice>,
    pub input_device_index: usize,
    pub use_default_input: bool,
    mouse_down_time: Option<Instant>,
    tap_candidate: bool,
    /// Updated whenever [`AudioInputSelector::draw`] runs (for hit-testing).
    pub last_button_rect: (f32, f32, f32, f32),
    /// Filled when drawing with `with_intensity_strip: true` — full-width track below the capture button.
    pub last_intensity_rect: (f32, f32, f32, f32),
}

impl Default for AudioInputSelector {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioInputSelector {
    pub fn new() -> Self {
        Self {
            enabled: true,
            show_menu: false,
            input_devices: Vec::new(),
            input_device_index: 0,
            use_default_input: false,
            mouse_down_time: None,
            tap_candidate: false,
            last_button_rect: (0.0, 0.0, 0.0, 0.0),
            last_intensity_rect: (0.0, 0.0, 0.0, 0.0),
        }
    }

    pub fn refresh_inputs_from_system(&mut self) {
        self.input_devices = crate::engine::audio::devices()
            .into_iter()
            .filter(|d| d.is_input)
            .collect();
    }

    /// Exposed for transcribe: reserve **lang** to the right of the capture control using the same size rule.
    pub fn capture_button_size_for_layout(
        wave_w: f32,
        wave_h: f32,
        viewport_w: f32,
        viewport_h: f32,
    ) -> f32 {
        Self::capture_button_size(wave_w, wave_h, viewport_w, viewport_h)
    }

    fn capture_button_size(_wave_w: f32, _wave_h: f32, _viewport_w: f32, _viewport_h: f32) -> f32 {
        #[cfg(target_os = "ios")]
        {
            let vm = _viewport_w.max(_viewport_h).max(1.0);
            (vm * 0.11).clamp(64.0, 200.0)
        }
        #[cfg(not(target_os = "ios"))]
        {
            (_wave_w.min(_wave_h) * 0.20).clamp(28.0, 64.0)
        }
    }

    /// Thickness of the optional waveform gain strip at the **bottom** of the wave band (below the green button).
    pub fn layout_intensity_strip_height(viewport_h: f32) -> f32 {
        (viewport_h * 0.042)
            .max(20.0)
            .min(40.0)
    }

    const INTENSITY_STRIP_TO_BUTTON_GAP: f32 = 6.0;

    /// Extra width to the right of the capture button, e.g. a **lang** control (`0` = capture centered alone).
    pub const TRAILING_TO_CAPTURE_GAP: f32 = 8.0;

    /// `with_intensity_strip`: if true, reserves a row at the bottom of the wave for a slider and moves
    /// the capture button up. Returns `(intensity_rect, button_rect, trailing_rect)`.
    /// `trailing_w > 0` places that width to the right of the green button and centers the **group** in the band.
    /// Intensity is `(0,0,0,0)` if `with_intensity_strip` is false; trailing is zero if `trailing_w <= 0`.
    pub fn layout_intensity_capture_trailing(
        wave_x0: f32,
        wave_y0: f32,
        wave_x1: f32,
        wave_y1: f32,
        viewport_w: f32,
        viewport_h: f32,
        with_intensity_strip: bool,
        trailing_w: f32,
        trailing_gap: f32,
    ) -> ((f32, f32, f32, f32), (f32, f32, f32, f32), (f32, f32, f32, f32)) {
        let wave_w = (wave_x1 - wave_x0).max(0.0);
        let wave_h = (wave_y1 - wave_y0).max(0.0);
        let btn_size = Self::capture_button_size(wave_w, wave_h, viewport_w, viewport_h);
        let tw = trailing_w.max(0.0);
        let g = if tw > 0.0 { trailing_gap.max(0.0) } else { 0.0 };
        let group_w = btn_size + g + tw;
        let cx = (wave_x0 + wave_x1) * 0.5;
        let max_left = (wave_x1 - group_w).max(wave_x0);
        let group_left = (cx - group_w * 0.5).clamp(wave_x0, max_left);
        let empty_t = (0.0, 0.0, 0.0, 0.0);
        let (btn_x0, trail_rect) = if tw > 0.0 {
            let cap0 = group_left;
            let tr = (cap0 + btn_size + g, 0.0, cap0 + group_w, 0.0);
            (cap0, (tr.0, tr.1, tr.2, tr.3))
        } else {
            (cx - btn_size * 0.5, empty_t)
        };
        if !with_intensity_strip {
            let btn_y1 = wave_y1 - 4.0;
            let btn_y0 = btn_y1 - btn_size;
            let btn_rect = (btn_x0, btn_y0, btn_x0 + btn_size, btn_y1);
            let trail = if tw > 0.0 {
                (trail_rect.0, btn_y0, trail_rect.2, btn_y1)
            } else {
                empty_t
            };
            return (empty_t, btn_rect, trail);
        }
        let sh = Self::layout_intensity_strip_height(viewport_h);
        let int_y1 = wave_y1;
        let int_y0 = (int_y1 - sh).max(wave_y0);
        let int_rect = (wave_x0, int_y0, wave_x1, int_y1);
        let btn_y1 = int_y0 - Self::INTENSITY_STRIP_TO_BUTTON_GAP;
        let btn_y0 = (btn_y1 - btn_size).max(wave_y0);
        let btn_rect = (btn_x0, btn_y0, btn_x0 + btn_size, btn_y1);
        let trail = if tw > 0.0 {
            (trail_rect.0, btn_y0, trail_rect.2, btn_y1)
        } else {
            empty_t
        };
        (int_rect, btn_rect, trail)
    }

    /// Same as [`Self::layout_intensity_capture_trailing`]`(..., 0, 0)` — capture centered, no trailing slot.
    pub fn layout_intensity_and_capture(
        wave_x0: f32,
        wave_y0: f32,
        wave_x1: f32,
        wave_y1: f32,
        viewport_w: f32,
        viewport_h: f32,
        with_intensity_strip: bool,
    ) -> ((f32, f32, f32, f32), (f32, f32, f32, f32)) {
        let (a, b, _) = Self::layout_intensity_capture_trailing(
            wave_x0,
            wave_y0,
            wave_x1,
            wave_y1,
            viewport_w,
            viewport_h,
            with_intensity_strip,
            0.0,
            0.0,
        );
        (a, b)
    }

    /// Place a square capture control at the bottom center of the waveform band.
    /// `viewport_w` / `viewport_h` are the full frame size in pixels (for a large touch target on iOS).
    pub fn layout_button_rect(
        wave_x0: f32,
        wave_y0: f32,
        wave_x1: f32,
        wave_y1: f32,
        viewport_w: f32,
        viewport_h: f32,
    ) -> (f32, f32, f32, f32) {
        let (_, b) = Self::layout_intensity_and_capture(
            wave_x0,
            wave_y0,
            wave_x1,
            wave_y1,
            viewport_w,
            viewport_h,
            false,
        );
        b
    }

    fn point_in_rect(px: f32, py: f32, r: (f32, f32, f32, f32)) -> bool {
        px >= r.0 && px <= r.2 && py >= r.1 && py <= r.3
    }

    /// Call each frame while the pointer is down to open the device menu after a hold (like AudioRelay).
    pub fn tick_hold_opens_menu(&mut self) {
        if let Some(t) = self.mouse_down_time {
            if !self.show_menu && t.elapsed() >= HOLD_DURATION {
                self.show_menu = true;
                self.mouse_down_time = None;
                self.tap_candidate = false;
            }
        }
    }

    fn input_menu_geometry(layout_left: usize, layout_width: usize) -> (usize, usize, usize) {
        let column_width = ((layout_width as f32 * MENU_COLUMN_WIDTH_RATIO).max(200.0)) as usize;
        let left_x = layout_left + layout_width.saturating_sub(column_width) / 2;
        let menu_y = MENU_PADDING as usize;
        (left_x, column_width, menu_y)
    }

    /// While the menu is open: handle a press (same semantics as AudioRelay — picks on down).
    /// `layout` is the safe-area rectangle in pixels: `(left, top, width, height)`.
    pub fn on_menu_pointer_down(
        &mut self,
        mx: f32,
        my: f32,
        layout: (f32, f32, f32, f32),
    ) -> AudioInputMenuDown {
        let (l, t, lw, _lh) = layout;
        let w = lw.max(1.0) as usize;
        let left_base = l.max(0.0) as usize;
        let top_base = t.max(0.0) as usize;
        let (left_x, column_width, menu_y_rel) = Self::input_menu_geometry(left_base, w);
        let menu_y = top_base + menu_y_rel;
        let mx = mx as usize;
        let my = my as usize;
        if mx < left_x || mx >= left_x + column_width {
            return AudioInputMenuDown::Dismiss;
        }
        let item_height = MENU_ITEM_HEIGHT as usize;
        let title_bottom = menu_y + item_height;
        if my < title_bottom {
            return AudioInputMenuDown::DismissInColumn;
        }
        let before = (self.use_default_input, self.input_device_index);
        if !self.apply_input_column_click(my, menu_y, item_height) {
            return AudioInputMenuDown::DismissInColumn;
        }
        let selection_changed = before != (self.use_default_input, self.input_device_index);
        AudioInputMenuDown::Pick { selection_changed }
    }

    /// `pointer_down` on the capture control (menu must be handled separately via [`Self::on_menu_pointer_down`]).
    /// Returns `true` if the press started on the capture button (consumes event for other UI).
    pub fn on_capture_pointer_down(&mut self, mx: f32, my: f32) -> bool {
        if Self::point_in_rect(mx, my, self.last_button_rect) {
            self.mouse_down_time = Some(Instant::now());
            self.tap_candidate = true;
            return true;
        }
        false
    }

    /// `pointer_up`: when the menu is open, relay keeps the menu up on release; selection happens on `pointer_down`.
    pub fn on_pointer_up(&mut self, mx: f32, my: f32) -> AudioInputSelectorUp {
        if self.show_menu {
            self.mouse_down_time = None;
            self.tap_candidate = false;
            return AudioInputSelectorUp::None;
        }
        let hold = self
            .mouse_down_time
            .map(|t| t.elapsed())
            .unwrap_or(Duration::ZERO);
        self.mouse_down_time = None;
        if hold >= HOLD_DURATION || !self.tap_candidate {
            self.tap_candidate = false;
            return AudioInputSelectorUp::None;
        }
        self.tap_candidate = false;
        if Self::point_in_rect(mx, my, self.last_button_rect) {
            self.enabled = !self.enabled;
            return AudioInputSelectorUp::CaptureToggled(self.enabled);
        }
        AudioInputSelectorUp::None
    }

    fn apply_input_column_click(&mut self, mouse_y: usize, menu_y: usize, item_height: usize) -> bool {
        let default_y = menu_y + item_height + 5;
        if mouse_y >= default_y && mouse_y < default_y + item_height {
            self.use_default_input = true;
            return true;
        }
        let first_device_y = default_y + item_height + 5;
        if mouse_y >= first_device_y {
            let device_index = (mouse_y - first_device_y) / (item_height + 5);
            if device_index < self.input_devices.len() {
                self.use_default_input = false;
                self.input_device_index = device_index;
                return true;
            }
        }
        false
    }

    /// Draw the capture button or the input selection overlay. Updates [`AudioInputSelector::last_button_rect`].
    /// `safe_layout` is `(left, top, width, height)` in pixels; the menu is centered in that region.
    pub fn draw(
        &mut self,
        state: &mut EngineState,
        font: &Font,
        wave_rect: (f32, f32, f32, f32),
        safe_layout: (f32, f32, f32, f32),
        with_intensity_strip: bool,
    ) {
        self.draw_with_trailing(
            state,
            font,
            wave_rect,
            safe_layout,
            with_intensity_strip,
            0.0,
        );
    }

    /// Same as [`Self::draw`], but reserves a right-side slot so external UI (e.g. `lang`) can sit
    /// beside capture without overlap; capture hit-target follows this layout.
    pub fn draw_with_trailing(
        &mut self,
        state: &mut EngineState,
        font: &Font,
        wave_rect: (f32, f32, f32, f32),
        safe_layout: (f32, f32, f32, f32),
        with_intensity_strip: bool,
        trailing_w: f32,
    ) {
        let shape = state.frame.shape();
        let full_h = shape[0] as f32;
        let full_w = shape[1] as f32;
        let height = shape[0] as usize;
        let width = shape[1] as usize;
        let (int_r, btn, _) = Self::layout_intensity_capture_trailing(
            wave_rect.0,
            wave_rect.1,
            wave_rect.2,
            wave_rect.3,
            full_w,
            full_h,
            with_intensity_strip,
            trailing_w,
            Self::TRAILING_TO_CAPTURE_GAP,
        );
        self.last_intensity_rect = int_r;
        self.last_button_rect = btn;

        if self.show_menu {
            self.draw_input_menu(state, font, width, height, safe_layout);
        } else {
            self.draw_button_pixels(state, btn, width, height);
        }
    }

    fn draw_button_pixels(&self, state: &mut EngineState, btn: (f32, f32, f32, f32), width: usize, height: usize) {
        let buffer = state.frame_buffer_mut();
        let x_start = btn.0.floor().max(0.0) as usize;
        let y_start = btn.1.floor().max(0.0) as usize;
        let x_end = btn.2.ceil().min(width as f32) as usize;
        let y_end = btn.3.ceil().min(height as f32) as usize;
        let border = BUTTON_BORDER_WIDTH as usize;
        let (r, g, b) = if self.enabled {
            (0_u8, 255_u8, 0_u8)
        } else {
            (100, 100, 100)
        };

        for y in y_start..y_end {
            if y >= height {
                break;
            }
            for x in x_start..x_end {
                if x >= width {
                    break;
                }
                let is_border = x < x_start + border
                    || x >= x_end.saturating_sub(border)
                    || y < y_start + border
                    || y >= y_end.saturating_sub(border);
                if self.enabled || is_border {
                    let idx = (y * width + x) * 4;
                    if idx + 3 < buffer.len() {
                        buffer[idx] = r;
                        buffer[idx + 1] = g;
                        buffer[idx + 2] = b;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
        }
    }

    fn draw_input_menu(
        &mut self,
        state: &mut EngineState,
        font: &Font,
        width: usize,
        height: usize,
        safe_layout: (f32, f32, f32, f32),
    ) {
        let buffer = state.frame_buffer_mut();
        let (l, t, lw, lh) = safe_layout;
        let layout_left = l.max(0.0) as usize;
        let layout_width = lw.max(1.0) as usize;
        let top_off = t.max(0.0) as usize;
        let (left_x, column_width, menu_y_rel) = Self::input_menu_geometry(layout_left, layout_width);
        let menu_y = top_off + menu_y_rel;
        let safe_bottom_px = (t + lh) as usize;

        let item_height = MENU_ITEM_HEIGHT as usize;

        self.draw_rect(buffer, width, height, left_x, menu_y, column_width, item_height, (0, 0, 0));
        self.draw_text(buffer, width, height, font, "Input", left_x + 10, menu_y + 15, 20.0, (255, 255, 255));

        let default_y = menu_y + item_height + 5;
        let default_color = if self.use_default_input {
            (0, 255, 0)
        } else {
            (0, 0, 0)
        };
        self.draw_rect(buffer, width, height, left_x, default_y, column_width, item_height, default_color);
        let text_color = if self.use_default_input { (0, 0, 0) } else { (255, 255, 255) };
        self.draw_text(buffer, width, height, font, "Default", left_x + 10, default_y + 15, 16.0, text_color);

        for (i, device) in self.input_devices.iter().enumerate() {
            let item_y = default_y + item_height + 5 + i * (item_height + 5);
            if item_y + item_height > safe_bottom_px.min(height) {
                break;
            }
            let item_color = if !self.use_default_input && i == self.input_device_index {
                (0, 255, 0)
            } else {
                (0, 0, 0)
            };
            self.draw_rect(buffer, width, height, left_x, item_y, column_width, item_height, item_color);
            let full_label = device.input_menu_label();
            let max_chars = 42_usize;
            let device_name = if full_label.chars().count() > max_chars {
                format!(
                    "{}...",
                    full_label.chars().take(max_chars.saturating_sub(3)).collect::<String>()
                )
            } else {
                full_label
            };
            let tcol = if !self.use_default_input && i == self.input_device_index {
                (0, 0, 0)
            } else {
                (255, 255, 255)
            };
            self.draw_text(buffer, width, height, font, &device_name, left_x + 10, item_y + 15, 14.0, tcol);
        }
    }

    fn draw_text(
        &self,
        buffer: &mut [u8],
        width: usize,
        height: usize,
        font: &Font,
        text: &str,
        x: usize,
        y: usize,
        font_size: f32,
        color: (u8, u8, u8),
    ) {
        let mut rasterizer = TextRasterizer::new(font.clone(), font_size);
        rasterizer.set_text(text.to_string());
        rasterizer.tick(width as f32, height as f32);

        for character in &rasterizer.characters {
            let char_x = x as i32 + character.x as i32;
            let char_y = y as i32 + character.y as i32;

            for bitmap_y in 0..character.metrics.height {
                for bitmap_x in 0..character.metrics.width {
                    let alpha = character.bitmap[bitmap_y * character.metrics.width + bitmap_x];

                    if alpha == 0 {
                        continue;
                    }

                    let px = char_x + bitmap_x as i32;
                    let py = char_y + bitmap_y as i32;

                    if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                        let idx = ((py as usize * width + px as usize) * 4) as usize;

                        let alpha_f = alpha as f32 / 255.0;
                        let inv_alpha = 1.0 - alpha_f;

                        buffer[idx + 0] =
                            ((color.0 as f32 * alpha_f) + (buffer[idx + 0] as f32 * inv_alpha)) as u8;
                        buffer[idx + 1] =
                            ((color.1 as f32 * alpha_f) + (buffer[idx + 1] as f32 * inv_alpha)) as u8;
                        buffer[idx + 2] =
                            ((color.2 as f32 * alpha_f) + (buffer[idx + 2] as f32 * inv_alpha)) as u8;
                    }
                }
            }
        }
    }

    fn draw_rect(
        &self,
        buffer: &mut [u8],
        width: usize,
        height: usize,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        color: (u8, u8, u8),
    ) {
        for dy in 0..h {
            let py = y + dy;
            if py >= height {
                break;
            }

            for dx in 0..w {
                let px = x + dx;
                if px >= width {
                    break;
                }

                let idx = (py * width + px) * 4;
                if idx + 3 < buffer.len() {
                    buffer[idx + 0] = color.0;
                    buffer[idx + 1] = color.1;
                    buffer[idx + 2] = color.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }

        let border_color = (200, 200, 200);
        for dx in 0..w {
            let px = x + dx;
            if px < width {
                if y < height {
                    let idx = (y * width + px) * 4;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = border_color.0;
                        buffer[idx + 1] = border_color.1;
                        buffer[idx + 2] = border_color.2;
                        buffer[idx + 3] = 0xff;
                    }
                }
                let bottom_y = y + h - 1;
                if bottom_y < height {
                    let idx = (bottom_y * width + px) * 4;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = border_color.0;
                        buffer[idx + 1] = border_color.1;
                        buffer[idx + 2] = border_color.2;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
        }
        for dy in 0..h {
            let py = y + dy;
            if py < height {
                if x < width {
                    let idx = (py * width + x) * 4;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = border_color.0;
                        buffer[idx + 1] = border_color.1;
                        buffer[idx + 2] = border_color.2;
                        buffer[idx + 3] = 0xff;
                    }
                }
                let right_x = x + w - 1;
                if right_x < width {
                    let idx = (py * width + right_x) * 4;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = border_color.0;
                        buffer[idx + 1] = border_color.1;
                        buffer[idx + 2] = border_color.2;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
        }
    }

    /// Resolve the current menu selection to a concrete device (for opening an [`AudioListener`]).
    pub fn resolved_input_device(&self) -> Option<AudioDevice> {
        if self.use_default_input {
            default_input()
        } else {
            self.input_devices.get(self.input_device_index).cloned()
        }
    }
}
