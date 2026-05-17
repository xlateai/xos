//! Global F3 menu (top-right): FPS, UI scale slider, opaque backing for readability.
//! Desktop: toggle with **F3** (or host binding). iOS: **three-finger long-press** on the
//! main viewport (same idea as Expo’s dev gesture); implemented in `XosViewportView.swift`.

use crate::engine::{
    frame_view_rect_norm, EngineState, F3_UI_SCALE_MAX_PERCENT, F3_UI_SCALE_MIN_PERCENT,
    FRAME_VIEW_ZOOM_MAX, FRAME_VIEW_ZOOM_MIN,
};
use crate::rasterizer::text::fonts::{self, FontFamily};
use crate::rasterizer::text::text_rasterization::TextRasterizer;

const REF_SHORT_EDGE: f32 = 920.0;
/// ~50% larger than the old 18px FPS label.
const BASE_FONT: f32 = 27.0;
/// Ctrl/Cmd + wheel sensitivity in scale-percent per wheel delta unit.
const SCALE_WHEEL_PERCENT_PER_UNIT: f32 = 16.0;
/// Monotonic settle rate for Ctrl/Cmd wheel scale smoothing (1/s).
const SCALE_SMOOTH_RATE: f32 = 22.0;
/// Shift + Ctrl/Cmd + wheel frame-zoom sensitivity.
const FRAME_ZOOM_WHEEL_RATE: f32 = 0.085;
/// Interaction fade decay rate for transient F3 visibility (1/s).
const F3_INTERACTION_FADE_DECAY: f32 = 3.2;
const FONT_OPTION_BASE_SIZE: f32 = 19.0;
const FONT_HEADER_BASE_SIZE: f32 = 17.0;
#[cfg(target_os = "ios")]
const IOS_MESH_TOGGLE_LABEL_BASE_SIZE: f32 = 16.0;

pub struct F3Menu {
    /// When false, FPS is still tracked but the menu is not drawn. Toggle with F3 (desktop) or
    /// three-finger long-press on iOS.
    pub visible: bool,
    fps_rasterizer: TextRasterizer,
    scale_rasterizer: TextRasterizer,
    font_header_rasterizer: TextRasterizer,
    font_option_rasterizers: Vec<TextRasterizer>,
    font_option_families: Vec<FontFamily>,
    active_font_family: FontFamily,
    #[cfg(target_os = "ios")]
    ios_mesh_toggle_rasterizer: TextRasterizer,
    #[cfg(target_os = "ios")]
    pub ios_mesh_enabled: bool,
    pub(crate) scale_dragging: bool,
    /// Smooth wheel-zoom target in F3 percent units.
    scale_zoom_target: f32,
    /// Current smooth scale value in F3 percent units (float; avoids integer oscillation).
    scale_zoom_value: f32,
    /// Current wheel-zoom spring velocity in percent / second.
    scale_zoom_velocity: f32,
    /// 0..1 transient alpha boost shown after zoom interactions when menu isn't pinned visible.
    interaction_fade: f32,
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
        let font = fonts::default_font();
        let active_font_family = fonts::default_font_family();
        let font_option_families = FontFamily::ALL.to_vec();
        let mut fps_rasterizer = TextRasterizer::new(font.clone(), BASE_FONT);
        fps_rasterizer.set_text("— FPS".to_string());
        let mut scale_rasterizer = TextRasterizer::new(font.clone(), BASE_FONT);
        scale_rasterizer.set_text("Scale: 100%".to_string());
        let mut font_header_rasterizer = TextRasterizer::new(font, FONT_HEADER_BASE_SIZE);
        font_header_rasterizer.set_text("Default font".to_string());
        let mut font_option_rasterizers = Vec::with_capacity(font_option_families.len());
        for family in &font_option_families {
            let option_font = match family {
                FontFamily::JetBrainsMono => fonts::jetbrains_mono(),
                FontFamily::Mplus1 => fonts::mplus1(),
                FontFamily::DotGothic16 => fonts::dot_gothic_16(),
            };
            let mut rasterizer = TextRasterizer::new(option_font, FONT_OPTION_BASE_SIZE);
            rasterizer.set_text(family.label().to_string());
            font_option_rasterizers.push(rasterizer);
        }
        #[cfg(target_os = "ios")]
        let mut ios_mesh_toggle_rasterizer =
            TextRasterizer::new(fonts::default_font(), IOS_MESH_TOGGLE_LABEL_BASE_SIZE);
        #[cfg(target_os = "ios")]
        ios_mesh_toggle_rasterizer.set_text("iOS Mesh (ios-xos)".to_string());
        Self {
            visible: false,
            fps_rasterizer,
            scale_rasterizer,
            font_header_rasterizer,
            font_option_rasterizers,
            font_option_families,
            active_font_family,
            #[cfg(target_os = "ios")]
            ios_mesh_toggle_rasterizer,
            #[cfg(target_os = "ios")]
            ios_mesh_enabled: false,
            scale_dragging: false,
            scale_zoom_target: 100.0,
            scale_zoom_value: 100.0,
            scale_zoom_velocity: 0.0,
            interaction_fade: 0.0,
            pointer_captured: false,
        }
    }

    #[inline]
    pub fn toggle_visible(&mut self) {
        self.visible = !self.visible;
    }

    #[inline]
    fn ui_scale(short_edge: f32) -> f32 {
        #[cfg(target_os = "ios")]
        {
            return ((short_edge / REF_SHORT_EDGE) * 1.755).clamp(0.47, 1.8);
        }
        #[cfg(not(target_os = "ios"))]
        {
            (short_edge / REF_SHORT_EDGE).clamp(0.28, 1.0)
        }
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
    font_header_top: f32,
    font_options_top: f32,
    font_option_height: f32,
    font_option_gap: f32,
    #[cfg(target_os = "ios")]
    toggle_left: f32,
    #[cfg(target_os = "ios")]
    toggle_right: f32,
    #[cfg(target_os = "ios")]
    toggle_top: f32,
    #[cfg(target_os = "ios")]
    toggle_bottom: f32,
    font_option_count: usize,
    button_left: f32,
    button_right: f32,
    button_top: f32,
    button_bottom: f32,
    step_left: f32,
    step_right: f32,
    step_top: f32,
    step_bottom: f32,
    minimap_left: f32,
    minimap_right: f32,
    minimap_top: f32,
    minimap_bottom: f32,
    show_minimap: bool,
    ui_scale: f32,
}

fn panel_geom(
    state: &EngineState,
    content_w: f32,
    us: f32,
    pad: f32,
    show_minimap: bool,
    font_option_count: usize,
) -> PanelGeom {
    let shape = state.frame.shape();
    let w = shape[1] as f32;
    let h = shape[0] as f32;
    let safe_top = state.frame.safe_region_boundaries.y1 * h;
    let line_gap = 6.0 * us;
    let slider_h = 14.0 * us;
    let line_h = 32.0 * us;
    let font_header_h = (24.0 * us).max(16.0);
    let font_option_h = (26.0 * us).max(16.0);
    let font_option_gap = (4.0 * us).max(2.0);
    #[cfg(target_os = "ios")]
    let toggle_h = (28.0 * us).max(18.0);
    let font_options_h = if font_option_count == 0 {
        0.0
    } else {
        font_option_count as f32 * font_option_h
            + (font_option_count as f32 - 1.0) * font_option_gap
    };
    let slider_w = (200.0 * us).max(120.0);
    let mini_h = (92.0 * us).max(56.0);
    let inner_w = content_w.max(slider_w);
    let panel_w = inner_w + pad * 2.0;
    let minimap_extra = if show_minimap { line_gap + mini_h } else { 0.0 };
    let font_extra = if font_option_count > 0 {
        line_gap + font_header_h + line_gap + font_options_h
    } else {
        0.0
    };
    #[cfg(target_os = "ios")]
    let toggle_extra = line_gap + toggle_h;
    #[cfg(not(target_os = "ios"))]
    let toggle_extra = 0.0_f32;
    let panel_h = pad
        + line_h
        + line_gap
        + line_h
        + line_gap
        + slider_h
        + font_extra
        + minimap_extra
        + toggle_extra
        + pad;
    let panel_left = w - panel_w - pad;
    let panel_top = safe_top + pad;
    let button_size = (22.0 * us).max(12.0);
    let button_gap = (6.0 * us).max(3.0);
    let step_right = panel_left + panel_w - pad;
    let step_left = step_right - button_size;
    let button_right = step_left - button_gap;
    let button_left = button_right - button_size;
    let button_top = panel_top + pad + ((line_h - button_size) * 0.5).max(0.0);
    let button_bottom = button_top + button_size;
    let step_top = button_top;
    let step_bottom = button_bottom;
    let slider_left = panel_left + pad;
    let slider_right = panel_left + panel_w - pad;
    let slider_top = panel_top + pad + line_h + line_gap + line_h + line_gap;
    let slider_bottom = slider_top + slider_h;
    let font_header_top = slider_bottom + line_gap;
    let font_header_bottom = font_header_top + font_header_h;
    let font_options_top = font_header_bottom + line_gap;
    let minimap_left = slider_left;
    let minimap_right = slider_right;
    let minimap_top = if font_option_count > 0 {
        font_options_top + font_options_h + line_gap
    } else {
        slider_bottom + line_gap
    };
    let minimap_bottom = minimap_top + mini_h;
    #[cfg(target_os = "ios")]
    let toggle_top = if show_minimap {
        minimap_bottom + line_gap
    } else if font_option_count > 0 {
        font_options_top + font_options_h + line_gap
    } else {
        slider_bottom + line_gap
    };
    #[cfg(target_os = "ios")]
    let toggle_bottom = toggle_top + toggle_h;
    PanelGeom {
        panel_left,
        panel_top,
        panel_w,
        panel_h,
        slider_left,
        slider_right,
        slider_top,
        slider_bottom,
        font_header_top,
        font_options_top,
        font_option_height: font_option_h,
        font_option_gap,
        #[cfg(target_os = "ios")]
        toggle_left: slider_left,
        #[cfg(target_os = "ios")]
        toggle_right: slider_right,
        #[cfg(target_os = "ios")]
        toggle_top,
        #[cfg(target_os = "ios")]
        toggle_bottom,
        font_option_count,
        button_left,
        button_right,
        button_top,
        button_bottom,
        step_left,
        step_right,
        step_top,
        step_bottom,
        minimap_left,
        minimap_right,
        minimap_top,
        minimap_bottom,
        show_minimap,
        ui_scale: us,
    }
}

#[inline]
pub fn f3_menu_boost_interaction_fade(state: &mut EngineState) {
    state.f3_menu.interaction_fade = 1.0;
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
    p.clamp(
        F3_UI_SCALE_MIN_PERCENT as i32,
        F3_UI_SCALE_MAX_PERCENT as i32,
    ) as u16
}

#[inline]
fn clamp_scale_percent_f32(v: f32) -> f32 {
    v.clamp(
        F3_UI_SCALE_MIN_PERCENT as f32,
        F3_UI_SCALE_MAX_PERCENT as f32,
    )
}

#[inline]
fn font_option_rect(geom: &PanelGeom, idx: usize) -> Option<(f32, f32, f32, f32)> {
    if idx >= geom.font_option_count {
        return None;
    }
    let y0 = geom.font_options_top + idx as f32 * (geom.font_option_height + geom.font_option_gap);
    let y1 = y0 + geom.font_option_height;
    Some((geom.slider_left, geom.slider_right, y0, y1))
}

fn sync_f3_default_font(menu: &mut F3Menu) {
    let current = fonts::default_font_family();
    if current == menu.active_font_family {
        return;
    }
    let active_font = fonts::default_font();
    let mut fps = TextRasterizer::new(active_font.clone(), menu.fps_rasterizer.font_size);
    fps.set_text(menu.fps_rasterizer.text.clone());
    menu.fps_rasterizer = fps;
    let mut scale = TextRasterizer::new(active_font.clone(), menu.scale_rasterizer.font_size);
    scale.set_text(menu.scale_rasterizer.text.clone());
    menu.scale_rasterizer = scale;
    let mut header = TextRasterizer::new(active_font, menu.font_header_rasterizer.font_size);
    header.set_text(menu.font_header_rasterizer.text.clone());
    menu.font_header_rasterizer = header;
    menu.active_font_family = current;
}

#[inline]
fn set_scale_immediate(state: &mut EngineState, percent: u16) {
    let p = percent.clamp(F3_UI_SCALE_MIN_PERCENT, F3_UI_SCALE_MAX_PERCENT);
    state.ui_scale_percent = p;
    state.f3_menu.scale_zoom_target = p as f32;
    state.f3_menu.scale_zoom_value = p as f32;
    state.f3_menu.scale_zoom_velocity = 0.0;
}

fn tick_scale_zoom_smoothing(state: &mut EngineState) {
    let dt = state.delta_time_seconds.clamp(1.0 / 240.0, 1.0 / 20.0);
    let target = clamp_scale_percent_f32(state.f3_menu.scale_zoom_target);
    state.f3_menu.scale_zoom_target = target;

    let current = state.f3_menu.scale_zoom_value;
    let delta = target - current;
    if delta.abs() < 0.01 {
        state.f3_menu.scale_zoom_value = target;
        state.ui_scale_percent = target.round() as u16;
        state.f3_menu.scale_zoom_velocity = 0.0;
        return;
    }

    // First-order low-pass toward target: smooth, fast, and strictly non-oscillatory.
    let alpha = 1.0 - (-SCALE_SMOOTH_RATE * dt).exp();
    let mut next = current + delta * alpha;
    // Hard snap in a tiny neighborhood so UI text/knob stop perfectly.
    if (target - next).abs() < 0.08 {
        next = target;
    }

    state.f3_menu.scale_zoom_velocity = 0.0;
    state.f3_menu.scale_zoom_value = next;
    state.ui_scale_percent = next.round() as u16;
}

/// Handle Ctrl/Cmd + wheel zoom on the F3 scale.
///
/// Positive `wheel_delta_y` zooms in, negative zooms out.
pub fn f3_menu_handle_zoom_scroll(state: &mut EngineState, wheel_delta_y: f32) -> bool {
    if wheel_delta_y.abs() < 1e-4 {
        return false;
    }
    f3_menu_boost_interaction_fade(state);
    let step = wheel_delta_y * SCALE_WHEEL_PERCENT_PER_UNIT;
    let target = clamp_scale_percent_f32(state.f3_menu.scale_zoom_target + step);
    state.f3_menu.scale_zoom_target = target;
    true
}

/// Handle Shift+Ctrl/Cmd + wheel frame zoom (zoom app frame, keep F3 overlay unscaled).
pub fn f3_menu_handle_frame_zoom_scroll(state: &mut EngineState, wheel_delta_y: f32) -> bool {
    if wheel_delta_y.abs() < 1e-4 {
        return false;
    }
    f3_menu_boost_interaction_fade(state);

    let old_zoom = state
        .frame_view_zoom_target
        .clamp(FRAME_VIEW_ZOOM_MIN, FRAME_VIEW_ZOOM_MAX);
    let scale = (wheel_delta_y * FRAME_ZOOM_WHEEL_RATE).exp();
    let new_zoom = (old_zoom * scale).clamp(FRAME_VIEW_ZOOM_MIN, FRAME_VIEW_ZOOM_MAX);

    let shape = state.frame.shape();
    let w = (shape[1].max(1)) as f32;
    let h = (shape[0].max(1)) as f32;
    let nx = (state.mouse.x / w).clamp(0.0, 1.0);
    let ny = (state.mouse.y / h).clamp(0.0, 1.0);

    let old_view = 1.0 / old_zoom;
    let old_half = old_view * 0.5;
    let old_cx = state.frame_view_center_x.clamp(old_half, 1.0 - old_half);
    let old_cy = state.frame_view_center_y.clamp(old_half, 1.0 - old_half);
    let old_left = old_cx - old_half;
    let old_top = old_cy - old_half;
    let world_x = old_left + nx * old_view;
    let world_y = old_top + ny * old_view;

    let new_view = 1.0 / new_zoom;
    let mut new_left = world_x - nx * new_view;
    let mut new_top = world_y - ny * new_view;
    let max_left = (1.0 - new_view).max(0.0);
    let max_top = (1.0 - new_view).max(0.0);
    new_left = new_left.clamp(0.0, max_left);
    new_top = new_top.clamp(0.0, max_top);

    state.frame_view_center_x = new_left + new_view * 0.5;
    state.frame_view_center_y = new_top + new_view * 0.5;
    // Apply immediately so paused/event-driven redraws (e.g. mouse move) cannot advance zoom.
    state.frame_view_zoom = new_zoom;
    state.frame_view_zoom_target = new_zoom;
    state.frame_view_zoom_velocity = 0.0;
    true
}

/// Match rasterized label widths to [`tick_f3_menu`] so hit-testing uses the same panel and slider as drawing.
fn measure_f3_panel(state: &mut EngineState) -> Option<(PanelGeom, f32)> {
    let shape = state.frame.shape();
    let width = shape[1] as f32;
    let height = shape[0] as f32;
    if width < 1.0 || height < 1.0 {
        return None;
    }
    let ui_scale = F3Menu::ui_scale(width.max(height));
    let pad = F3Menu::padding_scaled(ui_scale);
    let font_px = BASE_FONT * ui_scale;
    let fps_display = if let Some(f) = state.f3_fps_label_override {
        f.round().clamp(0.0, 999.0) as u32
    } else {
        (1.0 / state.delta_time_seconds.max(1e-5)).round().max(0.0) as u32
    };

    {
        let menu = &mut state.f3_menu;
        sync_f3_default_font(menu);
        menu.fps_rasterizer.set_font_size(font_px);
        menu.fps_rasterizer.set_text(format!("{fps_display} FPS"));
        menu.fps_rasterizer.tick(width, height);

        menu.scale_rasterizer.set_font_size(font_px);
        menu.scale_rasterizer
            .set_text(format!("Scale: {}%", state.ui_scale_percent));
        menu.scale_rasterizer.tick(width, height);

        menu.font_header_rasterizer
            .set_font_size(FONT_HEADER_BASE_SIZE * ui_scale);
        menu.font_header_rasterizer
            .set_text(format!("Default font: {}", fonts::default_font_name()));
        menu.font_header_rasterizer.tick(width, height);

        for (idx, r) in menu.font_option_rasterizers.iter_mut().enumerate() {
            r.set_font_size(FONT_OPTION_BASE_SIZE * ui_scale);
            if let Some(family) = menu.font_option_families.get(idx) {
                r.set_text(family.label().to_string());
            }
            r.tick(width, height);
        }
        #[cfg(target_os = "ios")]
        {
            menu.ios_mesh_toggle_rasterizer
                .set_font_size(IOS_MESH_TOGGLE_LABEL_BASE_SIZE * ui_scale);
            menu.ios_mesh_toggle_rasterizer
                .set_text("iOS Mesh (ios-xos)".to_string());
            menu.ios_mesh_toggle_rasterizer.tick(width, height);
        }
    }

    let _fps_w: f32 = state
        .f3_menu
        .fps_rasterizer
        .characters
        .iter()
        .map(|c| c.metrics.advance_width)
        .sum();
    let _label_w: f32 = state
        .f3_menu
        .scale_rasterizer
        .characters
        .iter()
        .map(|c| c.metrics.advance_width)
        .sum();
    let button_size = (22.0 * ui_scale).max(12.0);
    let button_gap = (6.0 * ui_scale).max(3.0);
    let slider_w = (200.0 * ui_scale).max(120.0);
    // Keep panel width stable so FPS/scale text width changes don't make the slider jitter horizontally.
    let content_w = slider_w.max(button_size * 2.0 + button_gap + 240.0 * ui_scale);
    let (_, _, vw, vh) = frame_view_rect_norm(state);
    let show_minimap = vw < 0.999 || vh < 0.999;
    let geom = panel_geom(
        state,
        content_w,
        ui_scale,
        pad,
        show_minimap,
        state.f3_menu.font_option_rasterizers.len(),
    );
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
    let on_button = mx >= geom.button_left
        && mx <= geom.button_right
        && my >= geom.button_top
        && my <= geom.button_bottom;
    let on_step = mx >= geom.step_left
        && mx <= geom.step_right
        && my >= geom.step_top
        && my <= geom.step_bottom;
    if on_button {
        f3_menu_boost_interaction_fade(state);
        let entering_pause = !state.paused;
        state.paused = !state.paused;
        if entering_pause {
            state.paused_frame_snapshot_pending = true;
        }
        state.f3_menu.scale_dragging = false;
        state.f3_menu.pointer_captured = true;
        return true;
    }
    if on_step {
        f3_menu_boost_interaction_fade(state);
        if state.paused {
            state.pending_step_ticks = state.pending_step_ticks.saturating_add(1);
        }
        state.f3_menu.scale_dragging = false;
        state.f3_menu.pointer_captured = true;
        return true;
    }
    #[cfg(target_os = "ios")]
    {
        let on_toggle = mx >= geom.toggle_left
            && mx <= geom.toggle_right
            && my >= geom.toggle_top
            && my <= geom.toggle_bottom;
        if on_toggle {
            f3_menu_boost_interaction_fade(state);
            state.f3_menu.ios_mesh_enabled = !state.f3_menu.ios_mesh_enabled;
            state.f3_menu.scale_dragging = false;
            state.f3_menu.pointer_captured = true;
            return true;
        }
    }
    for idx in 0..geom.font_option_count {
        let Some((x0, x1, y0, y1)) = font_option_rect(&geom, idx) else {
            continue;
        };
        let on_font_option = mx >= x0 && mx <= x1 && my >= y0 && my <= y1;
        if on_font_option {
            if let Some(family) = state.f3_menu.font_option_families.get(idx).copied() {
                f3_menu_boost_interaction_fade(state);
                fonts::set_default_font_family(family);
                state.f3_menu.pointer_captured = true;
                state.f3_menu.scale_dragging = false;
                return true;
            }
        }
    }
    if on_slider {
        f3_menu_boost_interaction_fade(state);
        state.f3_menu.scale_dragging = true;
        set_scale_immediate(state, percent_from_mouse_x(mx, &geom, knob_w));
    }
    state.f3_menu.pointer_captured = true;
    true
}

/// Returns true if the app should not receive this move (slider drag).
pub fn f3_menu_handle_mouse_move(state: &mut EngineState) -> bool {
    let menu = &mut state.f3_menu;
    if menu.scale_dragging && !state.mouse.is_left_clicking {
        menu.scale_dragging = false;
    }
    if !menu.visible || !menu.scale_dragging {
        return false;
    }
    let Some((geom, knob_w)) = measure_f3_panel(state) else {
        return false;
    };
    set_scale_immediate(state, percent_from_mouse_x(state.mouse.x, &geom, knob_w));
    f3_menu_boost_interaction_fade(state);
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
    tick_scale_zoom_smoothing(state);

    let dt = state.delta_time_seconds.clamp(1.0 / 240.0, 0.25);
    state.f3_menu.interaction_fade *= (-F3_INTERACTION_FADE_DECAY * dt).exp();
    if state.f3_menu.interaction_fade < 0.001 {
        state.f3_menu.interaction_fade = 0.0;
    }

    let overlay_alpha = if state.f3_menu.visible {
        1.0
    } else {
        state.f3_menu.interaction_fade
    };

    if overlay_alpha <= 0.0 {
        return;
    }

    let Some((geom, knob_w)) = measure_f3_panel(state) else {
        return;
    };
    let view_rect = frame_view_rect_norm(state);

    let shape = state.frame.shape();
    let width = shape[1] as f32;
    let height = shape[0] as f32;

    let buffer = state.frame.buffer_mut();
    let fw = width as usize;
    let fh = height as usize;

    // Opaque black panel behind text and slider.
    let panel_x1 = (geom.panel_left + geom.panel_w).ceil() as i32;
    let panel_y1 = (geom.panel_top + geom.panel_h).ceil() as i32;
    blend_rect(
        buffer,
        fw,
        fh,
        geom.panel_left as i32,
        geom.panel_top as i32,
        panel_x1,
        panel_y1,
        (0, 0, 0, (255.0 * overlay_alpha) as u8),
    );

    // Slider track
    let track_x1 = geom.slider_right.ceil() as i32;
    let track_y1 = geom.slider_bottom.ceil() as i32;
    blend_rect(
        buffer,
        fw,
        fh,
        geom.slider_left as i32,
        geom.slider_top as i32,
        track_x1,
        track_y1,
        (55, 55, 55, (255.0 * overlay_alpha) as u8),
    );

    // Play/pause button in top-right of panel.
    let btn_x0 = geom.button_left.floor() as i32;
    let btn_y0 = geom.button_top.floor() as i32;
    let btn_x1 = geom.button_right.ceil() as i32;
    let btn_y1 = geom.button_bottom.ceil() as i32;
    let btn_bg = if state.paused {
        (34, 128, 76, 0xff)
    } else {
        (120, 92, 26, 0xff)
    };
    blend_rect(
        buffer,
        fw,
        fh,
        btn_x0,
        btn_y0,
        btn_x1,
        btn_y1,
        (btn_bg.0, btn_bg.1, btn_bg.2, (255.0 * overlay_alpha) as u8),
    );

    // Step button (active only while paused).
    let step_x0 = geom.step_left.floor() as i32;
    let step_y0 = geom.step_top.floor() as i32;
    let step_x1 = geom.step_right.ceil() as i32;
    let step_y1 = geom.step_bottom.ceil() as i32;
    let step_bg = if state.paused {
        (44, 72, 124, 0xff)
    } else {
        (70, 70, 70, 0xff)
    };
    blend_rect(
        buffer,
        fw,
        fh,
        step_x0,
        step_y0,
        step_x1,
        step_y1,
        (
            step_bg.0,
            step_bg.1,
            step_bg.2,
            (255.0 * overlay_alpha) as u8,
        ),
    );

    // Step icon: rightward step-forward (triangle then right bar).
    let sw = (step_x1 - step_x0).max(4);
    let sh = (step_y1 - step_y0).max(4);
    let icon_col = if state.paused {
        (228, 240, 255, (255.0 * overlay_alpha) as u8)
    } else {
        (155, 155, 155, (255.0 * overlay_alpha) as u8)
    };
    let bar_x0 = step_x0 + (sw as f32 * 0.68) as i32;
    let bar_x1 = bar_x0 + ((sw as f32 * 0.11).round().max(1.0) as i32);
    let bar_y0 = step_y0 + (sh as f32 * 0.22) as i32;
    let bar_y1 = step_y0 + (sh as f32 * 0.78) as i32;
    blend_rect(buffer, fw, fh, bar_x0, bar_y0, bar_x1, bar_y1, icon_col);

    let tri_left = step_x0 + (sw as f32 * 0.26) as i32;
    let tri_right = step_x0 + (sw as f32 * 0.62) as i32;
    let tri_top = step_y0 + (sh as f32 * 0.24) as i32;
    let tri_bottom = step_y0 + (sh as f32 * 0.76) as i32;
    let tri_mid = (tri_top + tri_bottom) / 2;
    for x in tri_left..tri_right {
        let t = (tri_right - 1 - x) as f32 / (tri_right - tri_left).max(1) as f32;
        let y0 = (tri_mid as f32 - t * (tri_mid - tri_top) as f32) as i32;
        let y1 = (tri_mid as f32 + t * (tri_bottom - tri_mid) as f32) as i32;
        blend_rect(buffer, fw, fh, x, y0, x + 1, y1 + 1, icon_col);
    }

    if state.paused {
        // Play icon (triangle)
        let w = (btn_x1 - btn_x0).max(2);
        let h = (btn_y1 - btn_y0).max(2);
        let left = btn_x0 + (w as f32 * 0.34) as i32;
        let right = btn_x0 + (w as f32 * 0.72) as i32;
        let top = btn_y0 + (h as f32 * 0.24) as i32;
        let bottom = btn_y0 + (h as f32 * 0.76) as i32;
        let mid = (top + bottom) / 2;
        for x in left..right {
            let t = (right - 1 - x) as f32 / (right - left).max(1) as f32;
            let y0 = (mid as f32 - t * (mid - top) as f32) as i32;
            let y1 = (mid as f32 + t * (bottom - mid) as f32) as i32;
            blend_rect(
                buffer,
                fw,
                fh,
                x,
                y0,
                x + 1,
                y1 + 1,
                (235, 245, 235, (255.0 * overlay_alpha) as u8),
            );
        }
    } else {
        // Pause icon (two vertical bars)
        let w = (btn_x1 - btn_x0).max(4);
        let h = (btn_y1 - btn_y0).max(4);
        let bar_w = ((w as f32) * 0.18).round().max(1.0) as i32;
        let gap = ((w as f32) * 0.14).round().max(1.0) as i32;
        let x_start = btn_x0 + ((w - (bar_w * 2 + gap)) / 2);
        let y_start = btn_y0 + (h as f32 * 0.20) as i32;
        let y_end = btn_y0 + (h as f32 * 0.80) as i32;
        blend_rect(
            buffer,
            fw,
            fh,
            x_start,
            y_start,
            x_start + bar_w,
            y_end,
            (250, 240, 220, (255.0 * overlay_alpha) as u8),
        );
        blend_rect(
            buffer,
            fw,
            fh,
            x_start + bar_w + gap,
            y_start,
            x_start + bar_w + gap + bar_w,
            y_end,
            (250, 240, 220, (255.0 * overlay_alpha) as u8),
        );
    }

    // Knob position from percent (same inner range as [`percent_from_mouse_x`])
    let knob_w_i = knob_w as i32;
    let inner = (geom.slider_right - geom.slider_left - knob_w).max(1.0);
    let range = (F3_UI_SCALE_MAX_PERCENT - F3_UI_SCALE_MIN_PERCENT) as f32;
    let t = (state
        .ui_scale_percent
        .saturating_sub(F3_UI_SCALE_MIN_PERCENT) as f32)
        / range.max(1.0);
    let knob_left = geom.slider_left + t * inner;
    let knob_x1 = (knob_left + knob_w_i as f32).ceil() as i32;
    let knob_y0 = (geom.slider_top - 1.0).floor() as i32;
    let knob_y1 = (geom.slider_bottom + 1.0).ceil() as i32;
    blend_rect(
        buffer,
        fw,
        fh,
        knob_left as i32,
        knob_y0,
        knob_x1,
        knob_y1,
        (220, 220, 220, (255.0 * overlay_alpha) as u8),
    );

    // Font picker options.
    let selected_family = fonts::default_font_family();
    for idx in 0..geom.font_option_count {
        let Some((x0, x1, y0, y1)) = font_option_rect(&geom, idx) else {
            continue;
        };
        let is_selected = state
            .f3_menu
            .font_option_families
            .get(idx)
            .copied()
            .map(|f| f == selected_family)
            .unwrap_or(false);
        let bg = if is_selected {
            (46, 138, 72, (220.0 * overlay_alpha) as u8)
        } else {
            (48, 48, 48, (190.0 * overlay_alpha) as u8)
        };
        blend_rect(
            buffer,
            fw,
            fh,
            x0.floor() as i32,
            y0.floor() as i32,
            x1.ceil() as i32,
            y1.ceil() as i32,
            bg,
        );
    }

    if geom.show_minimap {
        let mx0 = geom.minimap_left.floor() as i32;
        let my0 = geom.minimap_top.floor() as i32;
        let mx1 = geom.minimap_right.ceil() as i32;
        let my1 = geom.minimap_bottom.ceil() as i32;
        let mini_w = (mx1 - mx0).max(1) as f32;
        let mini_h = (my1 - my0).max(1) as f32;

        blend_rect(
            buffer,
            fw,
            fh,
            mx0,
            my0,
            mx1,
            my1,
            (40, 40, 40, (255.0 * overlay_alpha) as u8),
        );

        let frame_aspect = (width / height.max(1.0)).max(1e-4);
        let mini_aspect = (mini_w / mini_h).max(1e-4);
        let (full_w, full_h) = if frame_aspect >= mini_aspect {
            (mini_w * 0.9, (mini_w * 0.9) / frame_aspect)
        } else {
            (mini_h * 0.9 * frame_aspect, mini_h * 0.9)
        };
        let full_x0 = (mx0 as f32 + (mini_w - full_w) * 0.5).round() as i32;
        let full_y0 = (my0 as f32 + (mini_h - full_h) * 0.5).round() as i32;
        let full_x1 = (full_x0 as f32 + full_w).round() as i32;
        let full_y1 = (full_y0 as f32 + full_h).round() as i32;
        blend_rect(
            buffer,
            fw,
            fh,
            full_x0,
            full_y0,
            full_x1,
            full_y1,
            (96, 96, 96, (210.0 * overlay_alpha) as u8),
        );

        let (vx, vy, vw, vh) = view_rect;
        let lens_x0 = (full_x0 as f32 + vx * full_w).round() as i32;
        let lens_y0 = (full_y0 as f32 + vy * full_h).round() as i32;
        let lens_x1 = (lens_x0 as f32 + vw * full_w).round() as i32;
        let lens_y1 = (lens_y0 as f32 + vh * full_h).round() as i32;
        blend_rect(
            buffer,
            fw,
            fh,
            lens_x0,
            lens_y0,
            lens_x1,
            lens_y1,
            (56, 124, 255, (190.0 * overlay_alpha) as u8),
        );
    }

    #[cfg(target_os = "ios")]
    {
        let tx0 = geom.toggle_left.floor() as i32;
        let ty0 = geom.toggle_top.floor() as i32;
        let tx1 = geom.toggle_right.ceil() as i32;
        let ty1 = geom.toggle_bottom.ceil() as i32;
        blend_rect(
            buffer,
            fw,
            fh,
            tx0,
            ty0,
            tx1,
            ty1,
            (46, 46, 52, (220.0 * overlay_alpha) as u8),
        );

        let toggle_w = (tx1 - tx0).max(8) as f32;
        let toggle_h = (ty1 - ty0).max(8) as f32;
        let switch_w = (toggle_w * 0.20).clamp(22.0, 40.0);
        let switch_h = (toggle_h * 0.55).clamp(14.0, 24.0);
        let switch_x1 = tx1 as f32 - (8.0 * geom.ui_scale);
        let switch_x0 = switch_x1 - switch_w;
        let switch_y0 = ty0 as f32 + (toggle_h - switch_h) * 0.5;
        let switch_y1 = switch_y0 + switch_h;
        let is_on = state.f3_menu.ios_mesh_enabled;
        let track_col = if is_on {
            (60, 150, 88, (230.0 * overlay_alpha) as u8)
        } else {
            (88, 88, 96, (220.0 * overlay_alpha) as u8)
        };
        blend_rect(
            buffer,
            fw,
            fh,
            switch_x0.floor() as i32,
            switch_y0.floor() as i32,
            switch_x1.ceil() as i32,
            switch_y1.ceil() as i32,
            track_col,
        );
        let knob_d = (switch_h - 4.0).max(8.0);
        let knob_x = if is_on {
            switch_x1 - knob_d - 2.0
        } else {
            switch_x0 + 2.0
        };
        blend_rect(
            buffer,
            fw,
            fh,
            knob_x.floor() as i32,
            (switch_y0 + 2.0).floor() as i32,
            (knob_x + knob_d).ceil() as i32,
            (switch_y1 - 2.0).ceil() as i32,
            (238, 238, 242, (255.0 * overlay_alpha) as u8),
        );
        blend_text(
            buffer,
            width,
            height,
            &state.f3_menu.ios_mesh_toggle_rasterizer,
            geom.toggle_left + (8.0 * geom.ui_scale),
            geom.toggle_top + ((geom.toggle_bottom - geom.toggle_top) * 0.23),
            (245, 245, 245),
            overlay_alpha,
        );
    }

    let ui_scale = F3Menu::ui_scale(width.max(height));
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
        overlay_alpha,
    );
    blend_text(
        buffer,
        width,
        height,
        &state.f3_menu.scale_rasterizer,
        label_origin_x,
        label_origin_y,
        (255, 255, 255),
        overlay_alpha,
    );
    blend_text(
        buffer,
        width,
        height,
        &state.f3_menu.font_header_rasterizer,
        geom.slider_left,
        geom.font_header_top,
        (210, 210, 210),
        overlay_alpha,
    );
    for (idx, text_rasterizer) in state.f3_menu.font_option_rasterizers.iter().enumerate() {
        let Some((x0, _x1, y0, y1)) = font_option_rect(&geom, idx) else {
            continue;
        };
        let text_w: f32 = text_rasterizer
            .characters
            .iter()
            .map(|c| c.metrics.advance_width)
            .sum();
        let tx =
            x0 + ((geom.slider_right - geom.slider_left - text_w) * 0.5).max(8.0 * geom.ui_scale);
        let ty = y0 + ((y1 - y0 - FONT_OPTION_BASE_SIZE * geom.ui_scale) * 0.35).max(2.0);
        blend_text(
            buffer,
            width,
            height,
            text_rasterizer,
            tx,
            ty,
            (245, 245, 245),
            overlay_alpha,
        );
    }
}

fn blend_text(
    buffer: &mut [u8],
    width: f32,
    height: f32,
    rasterizer: &TextRasterizer,
    origin_x: f32,
    origin_y: f32,
    rgb: (u8, u8, u8),
    overlay_alpha: f32,
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
                    let alpha_f = (alpha as f32 / 255.0) * overlay_alpha;
                    buffer[idx + 0] = ((rgb.0 as f32 * alpha_f)
                        + (buffer[idx + 0] as f32 * (1.0 - alpha_f)))
                        as u8;
                    buffer[idx + 1] = ((rgb.1 as f32 * alpha_f)
                        + (buffer[idx + 1] as f32 * (1.0 - alpha_f)))
                        as u8;
                    buffer[idx + 2] = ((rgb.2 as f32 * alpha_f)
                        + (buffer[idx + 2] as f32 * (1.0 - alpha_f)))
                        as u8;
                    buffer[idx + 3] = 0xff;
                }
            }
        }
    }
}

fn blend_rect(
    buffer: &mut [u8],
    width: usize,
    height: usize,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    rgba: (u8, u8, u8, u8),
) {
    if rgba.3 == 0 {
        return;
    }
    let sx = x0.max(0) as usize;
    let sy = y0.max(0) as usize;
    let ex = x1.max(0).min(width as i32) as usize;
    let ey = y1.max(0).min(height as i32) as usize;
    if sx >= ex || sy >= ey {
        return;
    }
    let a = rgba.3 as f32 / 255.0;
    let ia = 1.0 - a;
    for y in sy..ey {
        for x in sx..ex {
            let idx = (y * width + x) * 4;
            buffer[idx] = (rgba.0 as f32 * a + buffer[idx] as f32 * ia) as u8;
            buffer[idx + 1] = (rgba.1 as f32 * a + buffer[idx + 1] as f32 * ia) as u8;
            buffer[idx + 2] = (rgba.2 as f32 * a + buffer[idx + 2] as f32 * ia) as u8;
            buffer[idx + 3] = 0xff;
        }
    }
}
