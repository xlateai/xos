//! Global F3 menu (top-right): FPS, UI scale slider, opaque backing for readability.

use crate::engine::{
    EngineState, F3_UI_SCALE_MAX_PERCENT, F3_UI_SCALE_MIN_PERCENT,
};
use crate::rasterizer::fill_rect_buffer;
use crate::rasterizer::text::text_rasterization::TextRasterizer;

const REF_SHORT_EDGE: f32 = 920.0;
/// ~50% larger than the old 18px FPS label.
const BASE_FONT: f32 = 27.0;

pub struct F3Menu {
    /// When false, FPS is still tracked but the menu is not drawn. Toggle with F3 (desktop/web).
    pub visible: bool,
    fps_rasterizer: TextRasterizer,
    scale_rasterizer: TextRasterizer,
    pub(crate) scale_dragging: bool,
    /// True when the last mouse down was consumed by the F3 panel (skip matching mouse up for the app).
    pub(crate) pointer_captured: bool,
}

impl std::fmt::Debug for F3Menu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("F3Menu").finish_non_exhaustive()
    }
}

impl F3Menu {
    pub fn new() -> Self {
        let font_data = include_bytes!("../assets/JetBrainsMono-Regular.ttf");
        let font = fontdue::Font::from_bytes(font_data as &[u8], fontdue::FontSettings::default())
            .expect("Failed to load font for F3 menu");
        let mut fps_rasterizer = TextRasterizer::new(font.clone(), BASE_FONT);
        fps_rasterizer.set_text("— FPS".to_string());
        let mut scale_rasterizer = TextRasterizer::new(font, BASE_FONT);
        scale_rasterizer.set_text("Scale: 100%".to_string());
        Self {
            visible: false,
            fps_rasterizer,
            scale_rasterizer,
            scale_dragging: false,
            pointer_captured: false,
        }
    }

    #[inline]
    pub fn toggle_visible(&mut self) {
        self.visible = !self.visible;
    }

    #[inline]
    fn ui_scale(short_edge: f32) -> f32 {
        (short_edge / REF_SHORT_EDGE).clamp(0.28, 1.0)
    }

    #[inline]
    fn padding_scaled(scale: f32) -> f32 {
        (12.0_f32 * scale).max(6.0)
    }
}

struct PanelGeom {
    panel_left: f32,
    panel_top: f32,
    panel_w: f32,
    panel_h: f32,
    slider_left: f32,
    slider_right: f32,
    slider_top: f32,
    slider_bottom: f32,
}

fn panel_geom(state: &EngineState, content_w: f32, us: f32, pad: f32) -> PanelGeom {
    let shape = state.frame.tensor.shape();
    let w = shape[1] as f32;
    let h = shape[0] as f32;
    let safe_top = state.frame.safe_region_boundaries.y1 * h;
    let line_gap = 6.0 * us;
    let slider_h = 14.0 * us;
    let line_h = 32.0 * us;
    let slider_w = (200.0 * us).max(120.0);
    let inner_w = content_w.max(slider_w);
    let panel_w = inner_w + pad * 2.0;
    let panel_h = pad + line_h + line_gap + line_h + line_gap + slider_h + pad;
    let panel_left = w - panel_w - pad;
    let panel_top = safe_top + pad;
    let slider_left = panel_left + pad;
    let slider_right = slider_left + slider_w;
    let slider_top = panel_top + pad + line_h + line_gap + line_h + line_gap;
    let slider_bottom = slider_top + slider_h;
    PanelGeom {
        panel_left,
        panel_top,
        panel_w,
        panel_h,
        slider_left,
        slider_right,
        slider_top,
        slider_bottom,
    }
}

/// Map mouse x to percent so the **knob center** follows the cursor (same geometry as drawing).
fn percent_from_mouse_x(mx: f32, geom: &PanelGeom, knob_w: f32) -> u16 {
    let knob_w = knob_w.max(1.0);
    let track = (geom.slider_right - geom.slider_left).max(1.0);
    let inner = (track - knob_w).max(1.0);
    let half = knob_w * 0.5;
    let cx = mx.clamp(geom.slider_left + half, geom.slider_right - half);
    let t = (cx - geom.slider_left - half) / inner;
    let lo = F3_UI_SCALE_MIN_PERCENT as f32;
    let hi = F3_UI_SCALE_MAX_PERCENT as f32;
    let p = (lo + t * (hi - lo)).round() as i32;
    p.clamp(F3_UI_SCALE_MIN_PERCENT as i32, F3_UI_SCALE_MAX_PERCENT as i32) as u16
}

/// Match rasterized label widths to [`tick_f3_menu`] so hit-testing uses the same panel and slider as drawing.
fn measure_f3_panel(state: &mut EngineState) -> Option<(PanelGeom, f32)> {
    let shape = state.frame.tensor.shape();
    let width = shape[1] as f32;
    let height = shape[0] as f32;
    if width < 1.0 || height < 1.0 {
        return None;
    }
    let ui_scale = F3Menu::ui_scale(width.min(height));
    let pad = F3Menu::padding_scaled(ui_scale);
    let font_px = BASE_FONT * ui_scale;
    let fps_display = (1.0 / state.delta_time_seconds.max(1e-5)).round().max(0.0) as u32;

    {
        let menu = &mut state.f3_menu;
        menu.fps_rasterizer.set_font_size(font_px);
        menu
            .fps_rasterizer
            .set_text(format!("{fps_display} FPS"));
        menu.fps_rasterizer.tick(width, height);

        menu.scale_rasterizer.set_font_size(font_px);
        menu
            .scale_rasterizer
            .set_text(format!("Scale: {}%", state.ui_scale_percent));
        menu.scale_rasterizer.tick(width, height);
    }

    let fps_w: f32 = state
        .f3_menu
        .fps_rasterizer
        .characters
        .iter()
        .map(|c| c.metrics.advance_width)
        .sum();
    let label_w: f32 = state
        .f3_menu
        .scale_rasterizer
        .characters
        .iter()
        .map(|c| c.metrics.advance_width)
        .sum();
    let content_w = fps_w.max(label_w);
    let geom = panel_geom(state, content_w, ui_scale, pad);
    let knob_w = (12.0 * ui_scale).max(8.0).round().max(1.0);
    Some((geom, knob_w))
}

/// Returns true if the event should not be forwarded to the application.
pub fn f3_menu_handle_mouse_down(state: &mut EngineState) -> bool {
    if !state.f3_menu.visible {
        return false;
    }
    let Some((geom, knob_w)) = measure_f3_panel(state) else {
        return false;
    };
    let mx = state.mouse.x;
    let my = state.mouse.y;
    let in_panel = mx >= geom.panel_left
        && mx <= geom.panel_left + geom.panel_w
        && my >= geom.panel_top
        && my <= geom.panel_top + geom.panel_h;
    if !in_panel {
        return false;
    }
    let on_slider = mx >= geom.slider_left
        && mx <= geom.slider_right
        && my >= geom.slider_top
        && my <= geom.slider_bottom;
    if on_slider {
        state.f3_menu.scale_dragging = true;
        state.ui_scale_percent = percent_from_mouse_x(mx, &geom, knob_w);
    }
    state.f3_menu.pointer_captured = true;
    true
}

/// Returns true if the app should not receive this move (slider drag).
pub fn f3_menu_handle_mouse_move(state: &mut EngineState) -> bool {
    let menu = &mut state.f3_menu;
    if !menu.visible || !menu.scale_dragging {
        return false;
    }
    let Some((geom, knob_w)) = measure_f3_panel(state) else {
        return false;
    };
    state.ui_scale_percent = percent_from_mouse_x(state.mouse.x, &geom, knob_w);
    true
}

pub fn f3_menu_handle_mouse_up(state: &mut EngineState) -> bool {
    let menu = &mut state.f3_menu;
    menu.scale_dragging = false;
    let cap = menu.pointer_captured;
    menu.pointer_captured = false;
    cap
}

/// Update smoothed FPS and composite the F3 menu into the frame (after app + keyboard).
pub fn tick_f3_menu(state: &mut EngineState) {
    if !state.f3_menu.visible {
        return;
    }

    let Some((geom, knob_w)) = measure_f3_panel(state) else {
        return;
    };

    let shape = state.frame.tensor.shape();
    let width = shape[1] as f32;
    let height = shape[0] as f32;

    let buffer = state.frame.buffer_mut();
    let fw = width as usize;
    let fh = height as usize;

    // Opaque black panel behind text and slider.
    let panel_x1 = (geom.panel_left + geom.panel_w).ceil() as i32;
    let panel_y1 = (geom.panel_top + geom.panel_h).ceil() as i32;
    fill_rect_buffer(
        buffer,
        fw,
        fh,
        geom.panel_left as i32,
        geom.panel_top as i32,
        panel_x1,
        panel_y1,
        (0, 0, 0, 0xff),
    );

    // Slider track
    let track_x1 = geom.slider_right.ceil() as i32;
    let track_y1 = geom.slider_bottom.ceil() as i32;
    fill_rect_buffer(
        buffer,
        fw,
        fh,
        geom.slider_left as i32,
        geom.slider_top as i32,
        track_x1,
        track_y1,
        (55, 55, 55, 0xff),
    );

    // Knob position from percent (same inner range as [`percent_from_mouse_x`])
    let knob_w_i = knob_w as i32;
    let inner = (geom.slider_right - geom.slider_left - knob_w).max(1.0);
    let range = (F3_UI_SCALE_MAX_PERCENT - F3_UI_SCALE_MIN_PERCENT) as f32;
    let t = (state.ui_scale_percent.saturating_sub(F3_UI_SCALE_MIN_PERCENT) as f32) / range.max(1.0);
    let knob_left = geom.slider_left + t * inner;
    let knob_x1 = (knob_left + knob_w_i as f32).ceil() as i32;
    let knob_y0 = (geom.slider_top - 1.0).floor() as i32;
    let knob_y1 = (geom.slider_bottom + 1.0).ceil() as i32;
    fill_rect_buffer(
        buffer,
        fw,
        fh,
        knob_left as i32,
        knob_y0,
        knob_x1,
        knob_y1,
        (220, 220, 220, 0xff),
    );

    let ui_scale = F3Menu::ui_scale(width.min(height));
    let pad = F3Menu::padding_scaled(ui_scale);
    let fps_origin_x = geom.panel_left + pad;
    let fps_origin_y = geom.panel_top + pad;

    let label_origin_x = geom.panel_left + pad;
    let line_gap = 6.0 * ui_scale;
    let line_h = 32.0 * ui_scale;
    let label_origin_y = fps_origin_y + line_h + line_gap;

    blend_text(
        buffer,
        width,
        height,
        &state.f3_menu.fps_rasterizer,
        fps_origin_x,
        fps_origin_y,
        (0, 255, 0),
    );
    blend_text(
        buffer,
        width,
        height,
        &state.f3_menu.scale_rasterizer,
        label_origin_x,
        label_origin_y,
        (255, 255, 255),
    );
}

fn blend_text(
    buffer: &mut [u8],
    width: f32,
    height: f32,
    rasterizer: &TextRasterizer,
    origin_x: f32,
    origin_y: f32,
    rgb: (u8, u8, u8),
) {
    for character in &rasterizer.characters {
        let char_x = origin_x + character.x;
        let char_y = origin_y + character.y;
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
                if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                    let idx = ((py as u32 * width as u32 + px as u32) * 4) as usize;
                    let alpha_f = alpha as f32 / 255.0;
                    buffer[idx + 0] = ((rgb.0 as f32 * alpha_f)
                        + (buffer[idx + 0] as f32 * (1.0 - alpha_f))) as u8;
                    buffer[idx + 1] = ((rgb.1 as f32 * alpha_f)
                        + (buffer[idx + 1] as f32 * (1.0 - alpha_f))) as u8;
                    buffer[idx + 2] = ((rgb.2 as f32 * alpha_f)
                        + (buffer[idx + 2] as f32 * (1.0 - alpha_f))) as u8;
                    buffer[idx + 3] = 0xff;
                }
            }
        }
    }
}
