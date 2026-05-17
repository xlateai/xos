//! Native drawing surface for [`xos.ui.whiteboard`] (pan, zoom, strokes).

use crate::apps::text::TextApp;
use crate::engine::{EngineState, ScrollWheelUnit};

const MIN_POINT_DIST_SQ: f32 = 0.25;

#[derive(Clone)]
pub struct WhiteboardWidget {
    strokes: Vec<Vec<(f32, f32)>>,
    current_stroke: Vec<(f32, f32)>,
    was_drawing: bool,

    offset_x: f32,
    offset_y: f32,
    zoom: f32,

    draw_color: (u8, u8, u8),
    stroke_width: f32,

    editable: bool,
    scrollable_x: bool,
    scrollable_y: bool,
    zoomable: bool,

    python_viewport_norm: Option<(f32, f32, f32, f32)>,
    python_viewport: Option<(i32, i32, u32, u32)>,

    cached_canvas: Vec<u8>,
    cached_width: u32,
    cached_height: u32,
    cache_dirty: bool,
    cached_view: (f32, f32, f32),
}

impl WhiteboardWidget {
    pub fn new(
        draw_color: (u8, u8, u8),
        stroke_width: f32,
        editable: bool,
        scrollable_x: bool,
        scrollable_y: bool,
        zoomable: bool,
        python_viewport_norm: (f32, f32, f32, f32),
    ) -> Self {
        Self {
            strokes: Vec::new(),
            current_stroke: Vec::new(),
            was_drawing: false,
            offset_x: 0.0,
            offset_y: 0.0,
            zoom: 1.0,
            draw_color,
            stroke_width: stroke_width.max(0.5),
            editable,
            scrollable_x,
            scrollable_y,
            zoomable,
            python_viewport_norm: Some(python_viewport_norm),
            python_viewport: None,
            cached_canvas: Vec::new(),
            cached_width: 0,
            cached_height: 0,
            cache_dirty: true,
            cached_view: (0.0, 0.0, 1.0),
        }
    }

    pub fn set_draw_style(&mut self, draw_color: (u8, u8, u8), stroke_width: f32) {
        let w = stroke_width.max(0.5);
        if self.draw_color != draw_color || (self.stroke_width - w).abs() >= 0.01 {
            self.draw_color = draw_color;
            self.stroke_width = w;
            self.cache_dirty = true;
        }
    }

    pub fn set_interaction_flags(
        &mut self,
        editable: bool,
        scrollable_x: bool,
        scrollable_y: bool,
        zoomable: bool,
    ) {
        self.editable = editable;
        self.scrollable_x = scrollable_x;
        self.scrollable_y = scrollable_y;
        self.zoomable = zoomable;
    }

    pub fn sync_python_viewport_from_norm(&mut self, frame_w: f32, frame_h: f32) {
        let Some((nx1, ny1, nx2, ny2)) = self.python_viewport_norm else {
            return;
        };
        let px = TextApp::rounded_norm_rect_to_px(nx1, ny1, nx2, ny2, frame_w, frame_h);
        if self.python_viewport != Some(px) {
            self.python_viewport = Some(px);
            self.cache_dirty = true;
        }
    }

    pub fn sync_norm_rect(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) -> Result<(), &'static str> {
        if !(0.0..=1.0).contains(&x1)
            || !(0.0..=1.0).contains(&y1)
            || !(0.0..=1.0).contains(&x2)
            || !(0.0..=1.0).contains(&y2)
        {
            return Err("whiteboard rect coordinates must lie in [0.0, 1.0]");
        }
        if !(x2 > x1 && y2 > y1) {
            return Err("whiteboard rect must satisfy x2 > x1 and y2 > y1");
        }
        let next = (x1, y1, x2, y2);
        if self.python_viewport_norm != Some(next) {
            self.python_viewport_norm = Some(next);
            self.cache_dirty = true;
        }
        Ok(())
    }

    pub(crate) fn viewport_contains(&self, mx: f32, my: f32) -> bool {
        let Some((vx, vy, vw, vh)) = self.python_viewport else {
            return true;
        };
        let x0 = vx as f32;
        let y0 = vy as f32;
        let x1 = x0 + vw as f32;
        let y1 = y0 + vh as f32;
        mx >= x0 && mx < x1 && my >= y0 && my < y1
    }

    fn screen_to_local(&self, mx: f32, my: f32) -> (f32, f32) {
        let (vx, vy) = self
            .python_viewport
            .map(|(x, y, _, _)| (x as f32, y as f32))
            .unwrap_or((0.0, 0.0));
        (mx - vx, my - vy)
    }

    fn screen_to_world(&self, local_x: f32, local_y: f32) -> (f32, f32) {
        (
            (local_x - self.offset_x) / self.zoom,
            (local_y - self.offset_y) / self.zoom,
        )
    }

    fn world_to_screen(&self, x: f32, y: f32) -> (f32, f32) {
        (x * self.zoom + self.offset_x, y * self.zoom + self.offset_y)
    }

    fn ensure_cache(&mut self, width: u32, height: u32) {
        if width != self.cached_width || height != self.cached_height {
            self.cached_width = width;
            self.cached_height = height;
            self.cached_canvas = vec![0; (width * height * 4) as usize];
            self.cache_dirty = true;
        }
    }

    fn view_changed(&self) -> bool {
        self.cached_view != (self.offset_x, self.offset_y, self.zoom)
    }

    fn mark_view_cached(&mut self) {
        self.cached_view = (self.offset_x, self.offset_y, self.zoom);
    }

    fn rebuild_cache(&mut self, width: u32, height: u32) {
        let ox = self.offset_x;
        let oy = self.offset_y;
        let z = self.zoom;
        let stroke_width = self.stroke_width;
        let color = self.draw_color;
        let world_to_screen = move |x: f32, y: f32| (x * z + ox, y * z + oy);

        self.cached_canvas.fill(0);
        for stroke in &self.strokes {
            draw_stroke(
                &mut self.cached_canvas,
                width,
                height,
                stroke,
                &world_to_screen,
                stroke_width,
                color,
            );
        }
        if !self.current_stroke.is_empty() {
            draw_stroke(
                &mut self.cached_canvas,
                width,
                height,
                &self.current_stroke,
                &world_to_screen,
                stroke_width,
                color,
            );
        }
        self.mark_view_cached();
        self.cache_dirty = false;
    }

    fn append_point_if_moved(&mut self, world: (f32, f32)) -> bool {
        if let Some(last) = self.current_stroke.last() {
            let dx = world.0 - last.0;
            let dy = world.1 - last.1;
            if dx * dx + dy * dy < MIN_POINT_DIST_SQ {
                return false;
            }
        }
        self.current_stroke.push(world);
        true
    }

    fn draw_segment_on_cache(
        &mut self,
        width: u32,
        height: u32,
        p0: (f32, f32),
        p1: (f32, f32),
    ) {
        let (x0, y0) = self.world_to_screen(p0.0, p0.1);
        let (x1, y1) = self.world_to_screen(p1.0, p1.1);
        draw_line(
            &mut self.cached_canvas,
            width,
            height,
            x0,
            y0,
            x1,
            y1,
            self.stroke_width,
            self.draw_color,
        );
    }

    pub fn tick(&mut self, state: &EngineState) {
        let Some((_vx, _vy, vw, vh)) = self.python_viewport else {
            return;
        };
        let width = vw.max(1);
        let height = vh.max(1);
        self.ensure_cache(width, height);

        if self.view_changed() {
            self.cache_dirty = true;
        }

        let (local_x, local_y) = self.screen_to_local(state.mouse.x, state.mouse.y);
        let in_viewport = self.viewport_contains(state.mouse.x, state.mouse.y);

        let mut panned = false;
        if state.mouse.is_right_clicking && (in_viewport || self.was_drawing) {
            let prev_off = (self.offset_x, self.offset_y);
            if self.scrollable_x {
                self.offset_x += state.mouse.dx;
            }
            if self.scrollable_y {
                self.offset_y += state.mouse.dy;
            }
            panned = (self.offset_x, self.offset_y) != prev_off;
            if panned {
                self.cache_dirty = true;
            }
        }

        let mut added_point = false;
        if self.editable
            && state.mouse.is_left_clicking
            && (in_viewport || self.was_drawing)
            && self.current_stroke.len() < 10_000
        {
            let p = self.screen_to_world(local_x, local_y);
            if self.append_point_if_moved(p) {
                added_point = true;
            }
        }

        if self.was_drawing && !state.mouse.is_left_clicking {
            if self.current_stroke.is_empty() && in_viewport {
                let p = self.screen_to_world(local_x, local_y);
                self.current_stroke.push(p);
            }
            if !self.current_stroke.is_empty() {
                self.strokes
                    .push(std::mem::take(&mut self.current_stroke));
            }
        }

        self.was_drawing = self.editable && state.mouse.is_left_clicking;

        if self.cache_dirty {
            self.rebuild_cache(width, height);
        } else if added_point && !panned {
            let n = self.current_stroke.len();
            if n >= 2 {
                let p0 = self.current_stroke[n - 2];
                let p1 = self.current_stroke[n - 1];
                self.draw_segment_on_cache(width, height, p0, p1);
            } else if n == 1 {
                let (x, y) = self.world_to_screen(
                    self.current_stroke[0].0,
                    self.current_stroke[0].1,
                );
                draw_circle(
                    &mut self.cached_canvas,
                    width,
                    height,
                    x,
                    y,
                    self.stroke_width,
                    self.draw_color,
                );
            }
        }
    }

    pub fn paint_into_frame(
        &self,
        frame: &mut [u8],
        frame_w: u32,
        frame_h: u32,
        mouse_x: f32,
        mouse_y: f32,
        is_left_clicking: bool,
        is_right_clicking: bool,
    ) {
        let Some((vx, vy, vw, vh)) = self.python_viewport else {
            return;
        };
        let width = vw.max(1);
        let height = vh.max(1);
        blit_rgba_patch(
            frame,
            frame_w,
            frame_h,
            vx,
            vy,
            width,
            height,
            &self.cached_canvas,
        );

        if self.viewport_contains(mouse_x, mouse_y) {
            let (local_x, local_y) = self.screen_to_local(mouse_x, mouse_y);
            draw_cursor_dot_on_frame(
                frame,
                frame_w,
                frame_h,
                vx + local_x.round() as i32,
                vy + local_y.round() as i32,
                is_left_clicking,
                is_right_clicking,
            );
        }
    }

    pub fn on_scroll(&mut self, state: &EngineState, _dx: f32, dy: f32, _unit: ScrollWheelUnit) {
        if !self.zoomable {
            return;
        }
        if !self.viewport_contains(state.mouse.x, state.mouse.y) {
            return;
        }

        let factor = if dy > 0.0 { 1.1 } else { 1.0 / 1.1 };
        let (local_x, local_y) = self.screen_to_local(state.mouse.x, state.mouse.y);
        let world_before = self.screen_to_world(local_x, local_y);
        self.zoom *= factor;
        let world_after = self.screen_to_world(local_x, local_y);
        if self.scrollable_x {
            self.offset_x += (world_after.0 - world_before.0) * self.zoom;
        }
        if self.scrollable_y {
            self.offset_y += (world_after.1 - world_before.1) * self.zoom;
        }
        self.cache_dirty = true;
    }
}

fn draw_stroke(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    stroke: &[(f32, f32)],
    world_to_screen: impl Fn(f32, f32) -> (f32, f32),
    stroke_width: f32,
    color: (u8, u8, u8),
) {
    if stroke.is_empty() {
        return;
    }
    if stroke.len() == 1 {
        let (x, y) = world_to_screen(stroke[0].0, stroke[0].1);
        draw_circle(pixels, width, height, x, y, stroke_width, color);
        return;
    }
    for stroke_pair in stroke.windows(2) {
        let (x0, y0) = world_to_screen(stroke_pair[0].0, stroke_pair[0].1);
        let (x1, y1) = world_to_screen(stroke_pair[1].0, stroke_pair[1].1);
        draw_line(pixels, width, height, x0, y0, x1, y1, stroke_width, color);
    }
}

fn draw_line(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    stroke_width: f32,
    color: (u8, u8, u8),
) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let steps = dx.abs().max(dy.abs()) as usize;

    if steps == 0 {
        draw_circle(pixels, width, height, x0, y0, stroke_width, color);
        return;
    }

    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let x = x0 + t * dx;
        let y = y0 + t * dy;
        draw_circle(pixels, width, height, x, y, stroke_width, color);
    }
}

fn draw_circle(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    cx: f32,
    cy: f32,
    radius: f32,
    color: (u8, u8, u8),
) {
    let radius_squared = radius * radius;
    let start_x = (cx - radius).floor() as i32;
    let end_x = (cx + radius).ceil() as i32;
    let start_y = (cy - radius).floor() as i32;
    let end_y = (cy + radius).ceil() as i32;

    for y in start_y..end_y {
        for x in start_x..end_x {
            if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
                continue;
            }
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            if dx * dx + dy * dy <= radius_squared {
                let i = ((y as u32 * width + x as u32) * 4) as usize;
                if i + 3 < pixels.len() {
                    pixels[i] = color.0;
                    pixels[i + 1] = color.1;
                    pixels[i + 2] = color.2;
                    pixels[i + 3] = 0xff;
                }
            }
        }
    }
}

fn blit_rgba_patch(
    dst: &mut [u8],
    frame_w: u32,
    frame_h: u32,
    dst_x: i32,
    dst_y: i32,
    patch_w: u32,
    patch_h: u32,
    src: &[u8],
) {
    let fw = frame_w as i32;
    let fh = frame_h as i32;
    let pw = patch_w as i32;
    let ph = patch_h as i32;
    for y in 0..ph {
        let fy = dst_y + y;
        if fy < 0 || fy >= fh {
            continue;
        }
        for x in 0..pw {
            let fx = dst_x + x;
            if fx < 0 || fx >= fw {
                continue;
            }
            let si = ((y as u32 * patch_w + x as u32) * 4) as usize;
            if si + 3 >= src.len() {
                continue;
            }
            let a = src[si + 3];
            if a == 0 {
                continue;
            }
            let di = ((fy as u32 * frame_w + fx as u32) * 4) as usize;
            if di + 3 >= dst.len() {
                continue;
            }
            dst[di] = src[si];
            dst[di + 1] = src[si + 1];
            dst[di + 2] = src[si + 2];
            dst[di + 3] = src[si + 3];
        }
    }
}

fn draw_cursor_dot_on_frame(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    left: bool,
    right: bool,
) {
    let color = if right {
        (255, 0, 0)
    } else if left {
        (0, 255, 0)
    } else {
        (255, 255, 255)
    };
    let radius = 3.0;
    let cx = x as f32;
    let cy = y as f32;
    let radius_squared = radius * radius;
    let start_x = (cx - radius).max(0.0) as u32;
    let end_x = (cx + radius).min(width as f32) as u32;
    let start_y = (cy - radius).max(0.0) as u32;
    let end_y = (cy + radius).min(height as f32) as u32;

    for y_ in start_y..end_y {
        for x_ in start_x..end_x {
            let dx = x_ as f32 - cx;
            let dy = y_ as f32 - cy;
            if dx * dx + dy * dy <= radius_squared {
                let i = ((y_ * width + x_) * 4) as usize;
                if i + 3 < pixels.len() {
                    pixels[i] = color.0;
                    pixels[i + 1] = color.1;
                    pixels[i + 2] = color.2;
                    pixels[i + 3] = 0xff;
                }
            }
        }
    }
}
