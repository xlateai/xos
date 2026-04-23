//! Language picker for the transcribe app: small **lang** control to the right of the capture button,
//! hold-to-open menu (same timing as the audio input menu), `en` / `ja` rows.

use crate::engine::EngineState;
use crate::rasterizer::text::text_rasterization::TextRasterizer;
use fontdue::Font;
use std::time::{Duration, Instant};

const HOLD_DURATION: Duration = Duration::from_millis(250);
const MENU_TOP_PAD: f32 = 20.0;
const MENU_ITEM_HEIGHT: f32 = 50.0;
const MENU_COLUMN_WIDTH_RATIO: f32 = 0.45;

/// `(code, label)` — Whisper language codes the engine accepts.
pub const TRANSCRIBE_LANGUAGES: &[(&str, &str)] = &[("en", "English"), ("ja", "Japanese")];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscribeLangMenuDown {
    Dismiss,
    DismissInColumn,
    /// Index into [`TRANSCRIBE_LANGUAGES`].
    Pick { index: usize, changed: bool },
}

pub struct TranscribeLanguageSelector {
    pub show_menu: bool,
    /// Index into [`TRANSCRIBE_LANGUAGES`].
    pub selected_index: usize,
    /// Hit-test: **lang** button (or menu column while open, handled separately).
    pub last_button_rect: (f32, f32, f32, f32),
    mouse_down_time: Option<Instant>,
    tap_candidate: bool,
}

impl Default for TranscribeLanguageSelector {
    fn default() -> Self {
        Self::new()
    }
}

impl TranscribeLanguageSelector {
    pub fn new() -> Self {
        Self {
            show_menu: false,
            selected_index: 0,
            last_button_rect: (0.0, 0.0, 0.0, 0.0),
            mouse_down_time: None,
            tap_candidate: false,
        }
    }

    /// Width to pass into [`AudioInputSelector::layout_intensity_capture_trailing`].
    pub fn trailing_width_for_layout(btn_size: f32, viewport_w: f32) -> f32 {
        let vm = viewport_w.max(1.0);
        // Keep this roughly square with capture so the two-button group feels centered and balanced.
        btn_size.max(48.0).min((vm * 0.16).min(120.0))
    }

    pub fn current_language_code(&self) -> &'static str {
        TRANSCRIBE_LANGUAGES
            .get(self.selected_index)
            .map(|(c, _)| *c)
            .unwrap_or("en")
    }

    pub fn set_selected_index(&mut self, i: usize) {
        if i < TRANSCRIBE_LANGUAGES.len() {
            self.selected_index = i;
        }
    }

    /// Match hold-open behavior to [`AudioInputSelector::tick_hold_opens_menu`].
    pub fn tick_hold_opens_menu(&mut self) {
        if let Some(t) = self.mouse_down_time {
            if !self.show_menu && t.elapsed() >= HOLD_DURATION {
                self.show_menu = true;
                self.mouse_down_time = None;
                self.tap_candidate = false;
            }
        }
    }

    fn point_in_rect(px: f32, py: f32, r: (f32, f32, f32, f32)) -> bool {
        px >= r.0 && px <= r.2 && py >= r.1 && py <= r.3
    }

    /// Pointer down on the **lang** button (not the menu). Returns `true` if consumed.
    pub fn on_button_pointer_down(&mut self, mx: f32, my: f32) -> bool {
        if Self::point_in_rect(mx, my, self.last_button_rect) {
            self.mouse_down_time = Some(Instant::now());
            self.tap_candidate = true;
            return true;
        }
        false
    }

    /// After [`Self::on_button_pointer_up`], call this if a short tap did not open via hold
    /// (menu still closed) to open the menu in one click.
    pub fn on_pointer_tap_opens_if_closed(&mut self) {
        if !self.show_menu {
            self.show_menu = true;
        }
    }

    /// Release on the **lang** button. If the menu was closed and the press was a short tap,
    /// set `open_menu` so the caller can call [`Self::on_pointer_tap_opens_if_closed`].
    pub fn on_button_pointer_up(&mut self, mx: f32, my: f32) -> bool {
        if self.show_menu {
            self.mouse_down_time = None;
            self.tap_candidate = false;
            return false;
        }
        let hold = self
            .mouse_down_time
            .map(|t| t.elapsed())
            .unwrap_or(Duration::ZERO);
        self.mouse_down_time = None;
        if hold >= HOLD_DURATION || !self.tap_candidate {
            self.tap_candidate = false;
            return false;
        }
        self.tap_candidate = false;
        if Self::point_in_rect(mx, my, self.last_button_rect) {
            return true;
        }
        false
    }

    /// While the language menu is open: hit-test (down = pick, same as audio input menu).
    pub fn on_menu_pointer_down(
        &mut self,
        mx: f32,
        my: f32,
        layout: (f32, f32, f32, f32),
    ) -> TranscribeLangMenuDown {
        let (l, t, lw, _lh) = layout;
        let w = lw.max(1.0) as usize;
        let left_base = l.max(0.0) as usize;
        let top_base = t.max(0.0) as usize;
        let (left_x, column_width, menu_y_rel) = Self::menu_geometry(left_base, w);
        let menu_y = top_base + menu_y_rel;
        let mx = mx as usize;
        let my = my as usize;
        if mx < left_x || mx >= left_x + column_width {
            return TranscribeLangMenuDown::Dismiss;
        }
        let item_height = MENU_ITEM_HEIGHT as usize;
        let title_bottom = menu_y + item_height;
        if my < title_bottom {
            return TranscribeLangMenuDown::DismissInColumn;
        }
        let before = self.selected_index;
        if !self.apply_menu_click(my, menu_y, item_height) {
            return TranscribeLangMenuDown::DismissInColumn;
        }
        let changed = before != self.selected_index;
        TranscribeLangMenuDown::Pick {
            index: self.selected_index,
            changed,
        }
    }

    fn apply_menu_click(&mut self, mouse_y: usize, menu_y: usize, item_height: usize) -> bool {
        let first_row_y = menu_y + item_height + 5;
        if mouse_y >= first_row_y {
            let idx = (mouse_y - first_row_y) / (item_height + 5);
            if idx < TRANSCRIBE_LANGUAGES.len() {
                self.selected_index = idx;
                return true;
            }
        }
        false
    }

    fn menu_geometry(layout_left: usize, layout_width: usize) -> (usize, usize, usize) {
        let column_width = ((layout_width as f32 * MENU_COLUMN_WIDTH_RATIO).max(200.0)) as usize;
        let left_x = layout_left + layout_width.saturating_sub(column_width) / 2;
        let menu_y = MENU_TOP_PAD as usize;
        (left_x, column_width, menu_y)
    }

    /// Draw the **lang** label button or the overlay menu. Updates [`Self::last_button_rect`].
    pub fn draw(
        &mut self,
        state: &mut EngineState,
        font: &Font,
        last_button_rect: (f32, f32, f32, f32),
        width: usize,
        height: usize,
        safe_layout: (f32, f32, f32, f32),
    ) {
        self.last_button_rect = last_button_rect;
        if self.show_menu {
            self.draw_menu(state, font, width, height, safe_layout);
        } else {
            self.draw_label_button(state, font, last_button_rect, width, height);
        }
    }

    fn draw_label_button(
        &self,
        state: &mut EngineState,
        font: &Font,
        btn: (f32, f32, f32, f32),
        width: usize,
        height: usize,
    ) {
        let buffer = state.frame_buffer_mut();
        let x0 = btn.0.floor().max(0.0) as usize;
        let y0 = btn.1.floor().max(0.0) as usize;
        let x1 = btn.2.ceil().min(width as f32) as usize;
        let y1 = btn.3.ceil().min(height as f32) as usize;
        for y in y0..y1 {
            for x in x0..x1 {
                if x < width {
                    let idx = (y * width + x) * 4;
                    if idx + 3 < buffer.len() {
                        let edge = x <= x0 + 2 || x + 2 >= x1 || y <= y0 + 2 || y + 2 >= y1;
                        let (r, g, b) = if edge { (180, 186, 196) } else { (32, 36, 44) };
                        buffer[idx] = r;
                        buffer[idx + 1] = g;
                        buffer[idx + 2] = b;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
        }
        let fs = ((btn.3 - btn.1) * 0.28).max(10.0).min(20.0);
        let text = "lang";
        let tw = (btn.2 - btn.0) * 0.5 - text.len() as f32 * fs * 0.22;
        let tx = btn.0 + tw.max(4.0);
        let ty = btn.1 + (btn.3 - btn.1) * 0.22;
        self.draw_text_pixels(
            buffer,
            width,
            height,
            font,
            text,
            tx as usize,
            ty as usize,
            fs,
            (235, 238, 242),
        );
    }

    fn draw_menu(
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
        let (left_x, column_width, menu_y_rel) = Self::menu_geometry(layout_left, layout_width);
        let menu_y = top_off + menu_y_rel;
        let safe_bottom_px = (t + lh) as usize;
        let item_h = MENU_ITEM_HEIGHT as usize;

        self.draw_rect(buffer, width, left_x, menu_y, column_width, item_h, (0, 0, 0));
        self.draw_text_pixels(
            buffer,
            width,
            height,
            font,
            "Language",
            left_x + 10,
            menu_y + 15,
            20.0,
            (255, 255, 255),
        );

        for (i, (code, label)) in TRANSCRIBE_LANGUAGES.iter().enumerate() {
            let item_y = menu_y + item_h + 5 + i * (item_h + 5);
            if item_y + item_h > safe_bottom_px.min(height) {
                break;
            }
            let sel = i == self.selected_index;
            let bg = if sel { (0, 200, 90) } else { (0, 0, 0) };
            let tcol = if sel { (0, 0, 0) } else { (255, 255, 255) };
            self.draw_rect(buffer, width, left_x, item_y, column_width, item_h, bg);
            let line = format!("{code} — {label}");
            self.draw_text_pixels(
                buffer,
                width,
                height,
                font,
                &line,
                left_x + 10,
                item_y + 15,
                16.0,
                tcol,
            );
        }
    }

    fn draw_rect(
        &self,
        buffer: &mut [u8],
        width: usize,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        color: (u8, u8, u8),
    ) {
        let height = buffer.len() / (width * 4);
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
                    buffer[idx] = color.0;
                    buffer[idx + 1] = color.1;
                    buffer[idx + 2] = color.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }
        let border = (200, 200, 200);
        for dx in 0..w {
            if x + dx < width && y < height {
                let i = (y * width + (x + dx)) * 4;
                if i + 3 < buffer.len() {
                    buffer[i] = border.0;
                    buffer[i + 1] = border.1;
                    buffer[i + 2] = border.2;
                }
            }
        }
    }

    fn draw_text_pixels(
        &self,
        buffer: &mut [u8],
        width: usize,
        _height: usize,
        font: &Font,
        text: &str,
        x: usize,
        y: usize,
        font_size: f32,
        color: (u8, u8, u8),
    ) {
        let w_f = width as f32;
        let h_f = (buffer.len() / (width * 4)) as f32;
        let mut r = TextRasterizer::new(font.clone(), font_size);
        r.set_text(text.to_string());
        r.tick(w_f, h_f);
        for character in &r.characters {
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
                    if px >= 0 && px < width as i32 && py >= 0 && py < h_f as i32 {
                        let idx = ((py as u32 * width as u32 + px as u32) * 4) as usize;
                        if idx + 3 < buffer.len() {
                            let a = alpha as f32 / 255.0;
                            let inv = 1.0 - a;
                            buffer[idx] = (color.0 as f32 * a + buffer[idx] as f32 * inv) as u8;
                            buffer[idx + 1] = (color.1 as f32 * a + buffer[idx + 1] as f32 * inv) as u8;
                            buffer[idx + 2] = (color.2 as f32 * a + buffer[idx + 2] as f32 * inv) as u8;
                            buffer[idx + 3] = 0xff;
                        }
                    }
                }
            }
        }
    }
}
