use super::f3_menu::F3Menu;

use crate::burn_raster;
use xos_tensor::{BurnTensor, WgpuDevice};
use crate::time::Instant;
use std::ptr::NonNull;

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
            y1: 0.08, // Top edge starts below Dynamic Island
            x2: 1.0,
            y2: 0.95, // Bottom edge ends above home indicator
        }
    }

    /// Build from UI corners in normalized `[0,1]` frame space (`x2`/`y2` are right/bottom edges).
    /// Clamps components and guarantees a positive-area rectangle (fallback until host sends real insets).
    pub fn from_clamped_normalized_corners(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        let mut x1 = x1.clamp(0.0, 1.0);
        let mut y1 = y1.clamp(0.0, 1.0);
        let mut x2 = x2.clamp(0.0, 1.0);
        let mut y2 = y2.clamp(0.0, 1.0);
        const EPS: f32 = 1e-4;
        if x2 <= x1 {
            x2 = (x1 + EPS).min(1.0);
            if x2 <= x1 {
                x1 = (x2 - EPS).max(0.0);
            }
        }
        if y2 <= y1 {
            y2 = (y1 + EPS).min(1.0);
            if y2 <= y1 {
                y1 = (y2 - EPS).max(0.0);
            }
        }
        Self { x1, y1, x2, y2 }
    }
}

/// Frame state: GPU RGBA [`BurnTensor`] `[height, width, 4]` plus CPU staging / mirror, and safe region.
#[derive(Debug)]
pub struct FrameState {
    /// Burn tensor, f32 in **0..=255** per channel (RGBA).
    pub tensor: BurnTensor<3>,
    device: WgpuDevice,
    width: u32,
    height: u32,
    cpu_staging: Vec<u8>,
    /// When set, `buffer_mut` / fills use this memory instead of `cpu_staging` (native + `pixels`).
    pixels_mirror: Option<(NonNull<u8>, usize)>,
    gpu_dirty: bool,
    cpu_dirty: bool,
    /// Safe region bounding rectangle for UI elements
    pub safe_region_boundaries: SafeRegionBoundingRectangle,
}

impl FrameState {
    /// Create a new FrameState with given dimensions and safe region (opaque black).
    pub fn new(width: u32, height: u32, safe_region: SafeRegionBoundingRectangle) -> Self {
        let device = WgpuDevice::default();
        let h = height as usize;
        let w = width as usize;
        let len = (width * height * 4) as usize;
        let mut cpu_staging = vec![0u8; len];
        for chunk in cpu_staging.chunks_exact_mut(4) {
            chunk.copy_from_slice(&[0, 0, 0, 0xff]);
        }
        let tensor = burn_raster::tensor_from_rgba_u8(&device, w, h, &cpu_staging);
        Self {
            tensor,
            device,
            width,
            height,
            cpu_staging,
            pixels_mirror: None,
            gpu_dirty: false,
            cpu_dirty: false,
            safe_region_boundaries: safe_region,
        }
    }

    /// # Safety
    /// Same contract as the former `FrameTensor::set_pixels_mirror_buffer`.
    #[allow(dead_code)]
    pub(crate) unsafe fn set_pixels_mirror_buffer(&mut self, ptr: *mut u8, len: usize) {
        debug_assert_eq!(len, (self.width * self.height * 4) as usize);
        self.pixels_mirror = Some((NonNull::new(ptr).expect("pixels mirror ptr"), len));
    }

    #[allow(dead_code)]
    pub(crate) fn clear_pixels_mirror_buffer(&mut self) {
        self.pixels_mirror = None;
    }

    fn staging_slice_mut(&mut self) -> &mut [u8] {
        if let Some((ptr, len)) = &self.pixels_mirror {
            unsafe { std::slice::from_raw_parts_mut(ptr.as_ptr(), *len) }
        } else {
            &mut self.cpu_staging
        }
    }

    fn staging_slice(&self) -> &[u8] {
        if let Some((ptr, len)) = &self.pixels_mirror {
            unsafe { std::slice::from_raw_parts(ptr.as_ptr(), *len) }
        } else {
            &self.cpu_staging
        }
    }

    #[inline]
    pub(crate) fn device(&self) -> &WgpuDevice {
        &self.device
    }

    #[inline]
    pub(crate) fn tensor_dims(&self) -> [usize; 3] {
        self.tensor.dims()
    }

    #[inline]
    pub(crate) fn burn_tensor(&self) -> &BurnTensor<3> {
        &self.tensor
    }

    pub(crate) fn set_burn_tensor(&mut self, t: BurnTensor<3>) {
        self.tensor = t;
        self.gpu_dirty = true;
        self.cpu_dirty = false;
    }

    pub(crate) fn ensure_gpu_from_cpu(&mut self) {
        if self.cpu_dirty {
            let w = self.width as usize;
            let h = self.height as usize;
            let (ptr, len) = match &self.pixels_mirror {
                Some((p, l)) => (p.as_ptr() as *const u8, *l),
                None => (self.cpu_staging.as_ptr(), self.cpu_staging.len()),
            };
            let staging = unsafe { std::slice::from_raw_parts(ptr, len) };
            self.tensor = burn_raster::tensor_from_rgba_u8(&self.device, w, h, staging);
            self.cpu_dirty = false;
        }
    }

    pub(crate) fn fill_solid_fast(&mut self, color: (u8, u8, u8, u8)) {
        let px = [color.0, color.1, color.2, color.3];
        let buf = self.staging_slice_mut();
        for chunk in buf.chunks_exact_mut(4) {
            chunk.copy_from_slice(&px);
        }
        self.cpu_dirty = true;
    }

    fn sync_tensor_to_cpu(&mut self) {
        let h = self.height as usize;
        let w = self.width as usize;
        let data = self.tensor.clone().into_data();
        let s = data.as_slice::<f32>().expect("frame f32");
        let buf = self.staging_slice_mut();
        for i in 0..(h * w) {
            let o = i * 4;
            buf[o] = s[o].clamp(0., 255.) as u8;
            buf[o + 1] = s[o + 1].clamp(0., 255.) as u8;
            buf[o + 2] = s[o + 2].clamp(0., 255.) as u8;
            buf[o + 3] = s[o + 3].clamp(0., 255.) as u8;
        }
    }

    /// Immutable RGBA bytes; syncs from GPU if needed.
    pub fn data(&mut self) -> &[u8] {
        if self.gpu_dirty {
            self.sync_tensor_to_cpu();
            self.gpu_dirty = false;
        }
        self.staging_slice()
    }

    /// Get mutable access to the frame buffer (zero-copy for rasterizer)
    pub fn buffer_mut(&mut self) -> &mut [u8] {
        if self.gpu_dirty {
            self.sync_tensor_to_cpu();
            self.gpu_dirty = false;
        }
        self.cpu_dirty = true;
        self.staging_slice_mut()
    }

    /// Get the frame shape `[height, width, 4]`
    pub fn shape(&self) -> Vec<usize> {
        vec![self.height as usize, self.width as usize, 4]
    }

    /// Resize the frame (opaque black).
    pub fn resize(&mut self, width: u32, height: u32) {
        *self = Self::new(width, height, self.safe_region_boundaries.clone());
    }

    /// Replace the inset used for layout / Python `safe_region` (e.g. host-driven safe area).
    pub fn set_safe_region_boundaries(&mut self, safe: SafeRegionBoundingRectangle) {
        self.safe_region_boundaries = safe;
    }

    /// Clear to opaque black (GPU + CPU).
    pub fn clear(&mut self) {
        let len = (self.width * self.height * 4) as usize;
        self.pixels_mirror = None;
        self.cpu_staging.clear();
        self.cpu_staging.resize(len, 0);
        for chunk in self.cpu_staging.chunks_exact_mut(4) {
            chunk.copy_from_slice(&[0, 0, 0, 0xff]);
        }
        self.tensor = burn_raster::tensor_from_rgba_u8(
            &self.device,
            self.width as usize,
            self.height as usize,
            &self.cpu_staging,
        );
        self.gpu_dirty = false;
        self.cpu_dirty = false;
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

/// Last-known host modifier keys (desktop / Web). Synced by each platform host before key routing.
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyboardModifiers {
    pub shift: bool,
    /// Command on macOS, Control on Windows/Linux — same notion as shortcut detection.
    pub command: bool,
    pub alt: bool,
}

#[derive(Debug)]
pub struct KeyboardState {
    pub onscreen: crate::ui::onscreen_keyboard::OnScreenKeyboard,
    pub modifiers: KeyboardModifiers,
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
    /// Number of one-tick step requests queued while paused.
    pub pending_step_ticks: u32,
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
    /// When set, the F3 overlay shows this value as “FPS” instead of `1 / delta_time_seconds`
    /// (e.g. remote viewer reports actual stream frame rate).
    pub f3_fps_label_override: Option<f32>,
    /// Python multi-editor hosts: last pointer down in embed **content** (above the visible OSK), screen px.
    /// Used to bootstrap the trackpad laser when there is no live cursor-mapping yet.
    pub embed_last_plain_click_screen: Option<(f32, f32)>,
    /// After a tap on the OSK trackpad strip, run a synthetic down/up at this screen point (laser) so
    /// Python can retarget focus (`xos.ui.Text`) without dragging the finger into the text area.
    pub embed_synthetic_click_screen: Option<(f32, f32)>,
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
    (percent.clamp(F3_UI_SCALE_MIN_PERCENT, F3_UI_SCALE_MAX_PERCENT) as f32) / 100.0
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

    #[inline]
    pub fn set_safe_region_boundaries(&mut self, safe: SafeRegionBoundingRectangle) {
        self.frame.set_safe_region_boundaries(safe);
    }
}

/// Updates [`EngineState::delta_time_seconds`] from wall-clock time since the previous tick.
/// The first tick uses `1.0 / 60.0` seconds so frame-independent logic has a reasonable initial step.
pub fn tick_frame_delta(engine_state: &mut EngineState, last_instant: &mut Option<Instant>) {
    let now = Instant::now();
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
    let target = engine_state
        .frame_view_zoom_target
        .clamp(FRAME_VIEW_ZOOM_MIN, FRAME_VIEW_ZOOM_MAX);
    engine_state.frame_view_zoom_target = target;
    // Keep frame zoom fully deterministic and event-driven (wheel/drag only).
    // Never animate in tick; this prevents redraw-driven drift (e.g. plain mouse move).
    engine_state.frame_view_zoom = target;
    engine_state.frame_view_zoom_velocity = 0.0;

    // Hard snap near full-frame zoom to avoid residual micro-zoom after wheel release.
    if (engine_state.frame_view_zoom_target - FRAME_VIEW_ZOOM_MIN).abs() < 0.0005
        && (engine_state.frame_view_zoom - FRAME_VIEW_ZOOM_MIN).abs() < 0.003
    {
        engine_state.frame_view_zoom = FRAME_VIEW_ZOOM_MIN;
        engine_state.frame_view_zoom_target = FRAME_VIEW_ZOOM_MIN;
        engine_state.frame_view_zoom_velocity = 0.0;
    }

    engine_state.frame_view_center_x = clamp_center_for_zoom(
        engine_state.frame_view_center_x,
        engine_state.frame_view_zoom,
    );
    engine_state.frame_view_center_y = clamp_center_for_zoom(
        engine_state.frame_view_center_y,
        engine_state.frame_view_zoom,
    );
}

/// Apply current frame-view zoom directly to the app frame buffer (before keyboard/F3 overlays).
pub fn apply_frame_view_zoom(engine_state: &mut EngineState) {
    if engine_state.frame_view_zoom <= FRAME_VIEW_ZOOM_MIN + 0.001 {
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
    engine_state.frame_view_center_x =
        clamp_center_for_zoom(engine_state.frame_view_center_x, zoom);
    engine_state.frame_view_center_y =
        clamp_center_for_zoom(engine_state.frame_view_center_y, zoom);
}

/// How [`Application::on_scroll`] reported vertical deltas should be interpreted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ScrollWheelUnit {
    /// Discrete line steps (mouse wheel notch): deltas are multiplied to pixel-like distances.
    Line,
    /// High-resolution delta (touchpad / smooth scrolling): deltas are logical pixels (~1∶1).
    #[default]
    Pixel,
}

pub trait Application {
    fn setup(&mut self, state: &mut EngineState) -> Result<(), String>;
    fn tick(&mut self, state: &mut EngineState);

    fn on_mouse_down(&mut self, _state: &mut EngineState) {}
    fn on_mouse_up(&mut self, _state: &mut EngineState) {}
    fn on_mouse_move(&mut self, _state: &mut EngineState) {}
    fn on_scroll(
        &mut self,
        _state: &mut EngineState,
        _delta_x: f32,
        _delta_y: f32,
        _unit: ScrollWheelUnit,
    ) {
    }
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
            if let Some(shortcut) =
                detect_shortcut(ch, special_key.command_held, special_key.shift_held)
            {
                self.on_key_shortcut(state, shortcut);
            }
        }
    }
    fn on_key_shortcut(
        &mut self,
        _state: &mut EngineState,
        _shortcut: crate::engine::keyboard::shortcuts::ShortcutAction,
    ) {
    }
    fn on_screen_size_change(&mut self, _state: &mut EngineState, _width: u32, _height: u32) {}

    /// Called when the window is closing or Ctrl+C requested exit — stop I/O that can block drop.
    fn prepare_shutdown(&mut self, _state: &mut EngineState) {}
}
