/// Rounded pixel viewport from normalized rect — shared by text/whiteboard UI widgets.
pub fn rounded_norm_rect_to_px(
    nx1: f32,
    ny1: f32,
    nx2: f32,
    ny2: f32,
    frame_w: f32,
    frame_h: f32,
) -> (i32, i32, u32, u32) {
    let fw = frame_w.max(1.0);
    let fh = frame_h.max(1.0);
    let xa = (nx1.clamp(0.0, 1.0) * fw).round() as i32;
    let ya = (ny1.clamp(0.0, 1.0) * fh).round() as i32;
    let xb = (nx2.clamp(0.0, 1.0) * fw).round() as i32;
    let yb = (ny2.clamp(0.0, 1.0) * fh).round() as i32;
    let vw = (xb.saturating_sub(xa)).max(1) as u32;
    let vh = (yb.saturating_sub(ya)).max(1) as u32;
    (xa, ya, vw, vh)
}
