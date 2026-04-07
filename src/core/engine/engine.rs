use super::f3_menu::F3Menu;

use crate::tensor::FrameTensor;

/// Safe region bounding rectangle for UI elements
/// 
/// Defines the safe rectangular area where content can be displayed without being
/// obscured by system UI elements (e.g., notches, home indicators, etc.).
/// 
/// Coordinates are normalized (0.0 to 1.0) relative to the full viewport dimensions.
/// The coordinate system uses:
/// - x1, x2: horizontal coordinates (left to right, 0.0 to 1.0)
/// - y1, y2: vertical coordinates (top to bottom, 0.0 to 1.0)
/// 
/// The rectangle is defined by two corner points:
/// - (x1, y1): top-left corner (minimum x, minimum y)
/// - (x2, y2): bottom-right corner (maximum x, maximum y)
/// 
/// For a full-screen safe region (no restrictions), use (0.0, 0.0, 1.0, 1.0).
#[derive(Debug, Clone)]
pub struct SafeRegionBoundingRectangle {
    /// Left edge of the safe region (minimum x, typically 0.0)
    pub x1: f32,
    /// Top edge of the safe region (minimum y, typically > 0.0 on devices with notches)
    pub y1: f32,
    /// Right edge of the safe region (maximum x, typically 1.0)
    pub x2: f32,
    /// Bottom edge of the safe region (maximum y, typically < 1.0 on devices with home indicators)
    pub y2: f32,
}

impl SafeRegionBoundingRectangle {
    /// Create safe region for non-iOS devices (full screen, no restrictions)
    /// Returns a rectangle covering the entire viewport: (0.0, 0.0, 1.0, 1.0)
    pub fn full_screen() -> Self {
        Self {
            x1: 0.0,
            y1: 0.0,
            x2: 1.0,
            y2: 1.0,
        }
    }

    /// Create safe region for iOS devices (iPhone 16 Pro safe area)
    /// 
    /// Accounts for:
    /// - Dynamic Island at the top (~59pt, normalized to ~0.069)
    /// - Home indicator at the bottom (~34pt, normalized to ~0.960)
    /// 
    /// Returns a rectangle that excludes these areas.
    /// For iPhone 16 Pro (393x852 points), this is approximately (0.0, 0.069, 1.0, 0.960)
    pub fn ios_iphone_16_pro() -> Self {
        // iPhone 16 Pro: 393x852 points
        // Top safe area: ~59pt from top (Dynamic Island)
        // Bottom safe area: ~34pt from bottom (home indicator)
        // Normalized: top ~0.069, bottom ~0.960
        Self {
            x1: 0.0,
            y1: 0.08,  // Top edge starts below Dynamic Island
            x2: 1.0,
            y2: 0.95,  // Bottom edge ends above home indicator
        }
    }
}

/// Frame state containing the pixel tensor and safe region information
#[derive(Debug)]
pub struct FrameState {
    /// The pixel buffer with shape [height, width, 4] for RGBA pixels
    pub tensor: FrameTensor,
    /// Safe region bounding rectangle for UI elements
    pub safe_region_boundaries: SafeRegionBoundingRectangle,
}

impl FrameState {
    /// Create a new FrameState with given dimensions and safe region
    pub fn new(width: u32, height: u32, safe_region: SafeRegionBoundingRectangle) -> Self {
        Self {
            tensor: FrameTensor::new(width, height),
            safe_region_boundaries: safe_region,
        }
    }

    /// Get mutable access to the frame buffer (zero-copy for rasterizer)
    pub fn buffer_mut(&mut self) -> &mut [u8] {
        self.tensor.buffer_mut()
    }

    /// Get the frame shape [height, width, 4]
    pub fn shape(&self) -> Vec<usize> {
        self.tensor.shape().to_vec()
    }

    /// Resize the frame to new dimensions (preserves safe region, as it's normalized)
    pub fn resize(&mut self, width: u32, height: u32) {
        self.tensor.resize(width, height);
    }

    /// Clear the frame buffer to opaque black.
    pub fn clear(&mut self) {
        self.tensor.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    Default,
    Text,
    ResizeHorizontal,
    ResizeVertical,
    ResizeDiagonalNE,
    ResizeDiagonalNW,
    Hand,
    Crosshair,
    Hidden,
}

#[derive(Debug)]
pub struct CursorStyleSetter {
    current_style: CursorStyle,
}

impl CursorStyleSetter {
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            current_style: CursorStyle::Default,
        }
    }

    #[inline(always)]
    pub fn get(&self) -> CursorStyle {
        self.current_style
    }

    #[inline(always)]
    fn set(&mut self, val: CursorStyle) {
        self.current_style = val;
    }
}


macro_rules! impl_cursor_style_setters {
    ($($variant:ident => $method:ident),* $(,)?) => {
        impl CursorStyleSetter {
            $(
                #[inline(always)]
                pub fn $method(&mut self) {
                    self.set(CursorStyle::$variant);
                }
            )*
        }
    };
}

impl_cursor_style_setters! {
    Default => default,
    Text => text,
    ResizeHorizontal => resize_horizontal,
    ResizeVertical => resize_vertical,
    ResizeDiagonalNE => resize_diagonal_ne,
    ResizeDiagonalNW => resize_diagonal_nw,
    Hand => hand,
    Crosshair => crosshair,
    Hidden => hidden,
}

#[derive(Debug)]
pub struct MouseState {
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
    pub is_left_clicking: bool,
    pub is_right_clicking: bool,
    pub style: CursorStyleSetter,
}

#[derive(Debug)]
pub struct KeyboardState {
    pub onscreen: crate::ui::onscreen_keyboard::OnScreenKeyboard,
}

#[derive(Debug)]
pub struct EngineState {
    /// Frame state containing pixel array and safe region boundaries
    pub frame: FrameState,
    pub mouse: MouseState,
    pub keyboard: KeyboardState,
    /// Global F3 menu (FPS + UI scale; drawn by the engine after each app tick).
    pub f3_menu: F3Menu,
    /// F3 UI scale (25–500%). Default 100% → multiplier [`EngineState::f3_ui_scale_multiplier`] is 1.0 (`percent/100`).
    pub ui_scale_percent: u16,
    /// Seconds since the previous `Application::tick` (set by the host immediately before each tick).
    /// The first tick uses `1.0 / 60.0` as a nominal step so simulations can use `delta_time_seconds` safely.
    pub delta_time_seconds: f32,
    /// Global simulation pause controlled by the F3 menu play/pause button.
    pub paused: bool,
    /// View zoom applied to the app-rendered frame before overlays (1.0 = full frame).
    pub frame_view_zoom: f32,
    /// Target view zoom used by smoothing.
    pub frame_view_zoom_target: f32,
    /// Smoothed velocity for frame view zoom.
    pub frame_view_zoom_velocity: f32,
    /// Normalized viewport center within source frame (0..1).
    pub frame_view_center_x: f32,
    /// Normalized viewport center within source frame (0..1).
    pub frame_view_center_y: f32,
}

/// F3 scale bar range (slider maps linearly to multiplier `percent / 100`).
pub const F3_UI_SCALE_MIN_PERCENT: u16 = 25;
pub const F3_UI_SCALE_MAX_PERCENT: u16 = 500;
pub const F3_UI_SCALE_DEFAULT_PERCENT: u16 = 100;
pub const FRAME_VIEW_ZOOM_MIN: f32 = 1.0;
pub const FRAME_VIEW_ZOOM_MAX: f32 = 24.0;

/// Global UI scale multiplier from F3: **25% → 0.25**, **100% → 1.0**, **500% → 5.0**.
#[inline]
pub fn f3_ui_scale_multiplier(percent: u16) -> f32 {
    (percent
        .clamp(F3_UI_SCALE_MIN_PERCENT, F3_UI_SCALE_MAX_PERCENT) as f32)
        / 100.0
}

impl EngineState {
    /// Same as [`f3_ui_scale_multiplier`]: `ui_scale_percent / 100` (25–500% → 0.25–5.0).
    #[inline]
    pub fn ui_scale_coefficient(&self) -> f32 {
        f3_ui_scale_multiplier(self.ui_scale_percent)
    }

    /// Same as [`f3_ui_scale_multiplier`] for this frame’s [`EngineState::ui_scale_percent`].
    #[inline]
    pub fn f3_ui_scale_multiplier(&self) -> f32 {
        f3_ui_scale_multiplier(self.ui_scale_percent)
    }

    /// Get mutable access to the frame buffer (zero-copy for CPU arrays)
    /// Panics if the array is on a non-CPU device
    pub fn frame_buffer_mut(&mut self) -> &mut [u8] {
        self.frame.buffer_mut()
    }

    /// Resize the frame to new dimensions
    pub fn resize_frame(&mut self, width: u32, height: u32) {
        self.frame.resize(width, height);
    }
}

/// Updates [`EngineState::delta_time_seconds`] from wall-clock time since the previous tick.
/// The first tick uses `1.0 / 60.0` seconds so frame-independent logic has a reasonable initial step.
pub fn tick_frame_delta(engine_state: &mut EngineState, last_instant: &mut Option<std::time::Instant>) {
    let now = std::time::Instant::now();
    engine_state.delta_time_seconds = last_instant
        .map(|prev| (now - prev).as_secs_f32())
        .unwrap_or(1.0 / 60.0);
    *last_instant = Some(now);
}

#[inline]
fn clamp_center_for_zoom(center: f32, zoom: f32) -> f32 {
    let view_span = (1.0 / zoom.max(FRAME_VIEW_ZOOM_MIN)).clamp(0.0, 1.0);
    let half = 0.5 * view_span;
    center.clamp(half, 1.0 - half)
}

/// Normalized source rectangle currently visible in the output frame.
/// Returns `(x, y, w, h)` in normalized `[0,1]` coordinates.
pub fn frame_view_rect_norm(engine_state: &EngineState) -> (f32, f32, f32, f32) {
    let zoom = engine_state
        .frame_view_zoom
        .clamp(FRAME_VIEW_ZOOM_MIN, FRAME_VIEW_ZOOM_MAX);
    let w = (1.0 / zoom).clamp(0.0, 1.0);
    let h = (1.0 / zoom).clamp(0.0, 1.0);
    let cx = clamp_center_for_zoom(engine_state.frame_view_center_x, zoom);
    let cy = clamp_center_for_zoom(engine_state.frame_view_center_y, zoom);
    (cx - w * 0.5, cy - h * 0.5, w, h)
}

/// Smoothly update frame zoom value toward its target.
pub fn tick_frame_view_zoom(engine_state: &mut EngineState) {
    let dt = engine_state.delta_time_seconds.clamp(1.0 / 240.0, 1.0 / 20.0);
    let target = engine_state
        .frame_view_zoom_target
        .clamp(FRAME_VIEW_ZOOM_MIN, FRAME_VIEW_ZOOM_MAX);
    engine_state.frame_view_zoom_target = target;

    let current = engine_state.frame_view_zoom;
    let x = current - target;
    let v = engine_state.frame_view_zoom_velocity;
    const OMEGA: f32 = 20.0;
    const ZETA: f32 = 0.84;

    if x.abs() < 0.0008 && v.abs() < 0.01 {
        engine_state.frame_view_zoom = target;
        engine_state.frame_view_zoom_velocity = 0.0;
    } else {
        let accel = -2.0 * ZETA * OMEGA * v - OMEGA * OMEGA * x;
        let mut new_v = v + accel * dt;
        let mut new_zoom = current + new_v * dt;
        let clamped = new_zoom.clamp(FRAME_VIEW_ZOOM_MIN, FRAME_VIEW_ZOOM_MAX);
        if (clamped - new_zoom).abs() > f32::EPSILON {
            new_zoom = clamped;
            new_v = 0.0;
        }
        engine_state.frame_view_zoom = new_zoom;
        engine_state.frame_view_zoom_velocity = new_v;
    }

    engine_state.frame_view_center_x = clamp_center_for_zoom(engine_state.frame_view_center_x, engine_state.frame_view_zoom);
    engine_state.frame_view_center_y = clamp_center_for_zoom(engine_state.frame_view_center_y, engine_state.frame_view_zoom);
}

/// Apply current frame-view zoom directly to the app frame buffer (before keyboard/F3 overlays).
pub fn apply_frame_view_zoom(engine_state: &mut EngineState) {
    if engine_state.frame_view_zoom <= FRAME_VIEW_ZOOM_MIN + 1e-4 {
        return;
    }

    let shape = engine_state.frame.shape();
    let h = shape[0];
    let w = shape[1];
    if h == 0 || w == 0 {
        return;
    }

    let (left, top, vw, vh) = frame_view_rect_norm(engine_state);
    let src = engine_state.frame.buffer_mut().to_vec();
    let dst = engine_state.frame.buffer_mut();

    for y in 0..h {
        let v = (y as f32 + 0.5) / h as f32;
        let sy = ((top + v * vh) * h as f32).floor() as isize;
        let sy = sy.clamp(0, (h as isize) - 1) as usize;
        for x in 0..w {
            let u = (x as f32 + 0.5) / w as f32;
            let sx = ((left + u * vw) * w as f32).floor() as isize;
            let sx = sx.clamp(0, (w as isize) - 1) as usize;
            let sidx = (sy * w + sx) * 4;
            let didx = (y * w + x) * 4;
            dst[didx..didx + 4].copy_from_slice(&src[sidx..sidx + 4]);
        }
    }
}

/// Pan the zoomed frame view by pixel deltas in output space.
/// Positive `dx`/`dy` follow mouse movement; view center shifts inversely (drag-to-pan).
pub fn frame_view_pan_by_pixels(
    engine_state: &mut EngineState,
    dx: f32,
    dy: f32,
    output_width: f32,
    output_height: f32,
) {
    let zoom = engine_state
        .frame_view_zoom
        .clamp(FRAME_VIEW_ZOOM_MIN, FRAME_VIEW_ZOOM_MAX);
    if zoom <= FRAME_VIEW_ZOOM_MIN + 1e-4 {
        return;
    }
    let w = output_width.max(1.0);
    let h = output_height.max(1.0);
    engine_state.frame_view_center_x -= dx / (w * zoom);
    engine_state.frame_view_center_y -= dy / (h * zoom);
    engine_state.frame_view_center_x = clamp_center_for_zoom(engine_state.frame_view_center_x, zoom);
    engine_state.frame_view_center_y = clamp_center_for_zoom(engine_state.frame_view_center_y, zoom);
}

pub trait Application {
    fn setup(&mut self, state: &mut EngineState) -> Result<(), String>;
    fn tick(&mut self, state: &mut EngineState);

    fn on_mouse_down(&mut self, _state: &mut EngineState) {}
    fn on_mouse_up(&mut self, _state: &mut EngineState) {}
    fn on_mouse_move(&mut self, _state: &mut EngineState) {}
    fn on_scroll(&mut self, _state: &mut EngineState, _delta_x: f32, _delta_y: f32) {}
    fn on_key_char(&mut self, _state: &mut EngineState, _ch: char) {}
    fn on_special_key(
        &mut self,
        state: &mut EngineState,
        special_key: crate::engine::keyboard::shortcuts::SpecialKeyEvent,
    ) {
        use crate::engine::keyboard::shortcuts::{detect_shortcut, NamedSpecialKey};

        if let Some(named) = special_key.named_key {
            match named {
                NamedSpecialKey::Backspace => {
                    self.on_key_char(state, '\u{8}');
                    return;
                }
                NamedSpecialKey::Enter => {
                    self.on_key_char(state, '\n');
                    return;
                }
                NamedSpecialKey::Escape => {
                    self.on_key_char(state, '\u{1b}');
                    return;
                }
                NamedSpecialKey::Tab => {
                    self.on_key_char(state, '\t');
                    return;
                }
                NamedSpecialKey::ArrowLeft => {
                    self.on_key_char(state, '\u{2190}');
                    return;
                }
                NamedSpecialKey::ArrowRight => {
                    self.on_key_char(state, '\u{2192}');
                    return;
                }
                NamedSpecialKey::ArrowUp => {
                    self.on_key_char(state, '\u{2191}');
                    return;
                }
                NamedSpecialKey::ArrowDown => {
                    self.on_key_char(state, '\u{2193}');
                    return;
                }
            }
        }

        if let Some(ch) = special_key.character {
            if let Some(shortcut) = detect_shortcut(ch, special_key.command_held, special_key.shift_held) {
                self.on_key_shortcut(state, shortcut);
            }
        }
    }
    fn on_key_shortcut(&mut self, _state: &mut EngineState, _shortcut: crate::engine::keyboard::shortcuts::ShortcutAction) {}
    fn on_screen_size_change(&mut self, _state: &mut EngineState, _width: u32, _height: u32) {}
}
