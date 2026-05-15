"""Click-to-zoom magnifier overlay for a target ``xos.ui.Text`` widget.

Behavior:

  * Hovering the cursor over a glyph draws a translucent highlight behind the
    collided hitbox(es), using the same ``xos.rasterizer.rects_filled`` /
    ``xos.geom.rect.containing`` + ``buffer`` raster pipeline as the lime
    backdrop in ``StudyApp.tick``.
  * Clicking on a hovered glyph opens a zoomed-in view of the selected
    character(s) on top of a darkened backdrop. The view is a panel that
    starts the size of the safe region.
  * While the magnifier is open the user can:
      - **Pan the panel** (the magnifier window itself, glyph and all) by
        left-click-dragging anywhere inside it. The panel — which contains
        the centered, zoomed glyph — moves around the viewport with the
        cursor, clamped so the whole panel always stays on-screen.
      - **Zoom** the rendered character two ways:
          * ``+`` / ``−`` buttons in the safe-region bottom row (input-
            independent, always available on touch-only devices).
          * Scroll wheel / trackpad (``positive dy`` = zoom out, ``negative
            dy`` = zoom in) when the platform delivers scroll events.
        Character zoom is independent of the panel — when the glyph grows
        larger than the panel the engine clips it to the panel's rect, so
        only the central region is visible (the user can zoom out to see
        the whole character).
      - **Dismiss** the overlay via the Close button in the same bottom row
        (clicks elsewhere never dismiss).
  * The panel starts at ``PANEL_DEFAULT_SCALE`` of the safe region — small
    enough to leave headroom inside the viewport so dragging visibly moves
    the magnifier — and is always clamped to the viewport ``[0,1]²``.
  * If a host-managed onscreen keyboard reference is supplied via the
    ``keyboard`` constructor argument, it is hidden whenever the magnifier
    opens (and again on any tick while open, so it stays hidden if something
    else re-shows it).

Coordinate conventions:

  * ``_panel_cx, _panel_cy`` — panel center in normalized viewport coords
    (same space as ``xos.ui.Text.x1`` etc.).
  * ``_panel_scale`` — uniform scale applied to the *safe-region* width/height
    to derive the panel size. ``1.0`` = panel matches the safe region (initial
    state). Values ``> 1.0`` grow the panel beyond the safe region up to the
    viewport edge; values ``< 1.0`` shrink it.
  * ``_glyph_scale`` — independent multiplier applied to the rendered glyph
    size inside the panel. This is what +/− and scroll modify.
"""

import xos


HOVER_HIGHLIGHT_COLOR = (*xos.color.CYAN, 0.8)
ZOOM_BACKDROP_COLOR = (0, 0, 0, 0.88)
ZOOM_FONT_SIZE_MULT = 8.0
HOVER_BUFFER_SCALE = 1.2

# Cached so we don't reallocate the same constant rect tensor every tick.
_FULL_FRAME_RECT = xos.tensor([0.0, 0.0, 1.0, 1.0], shape=(2, 2))


# Bottom button row geometry expressed in safe-region-local coords (each
# renormalized onto the viewport every tick via ``app.safe_region.renormalize``).
# Layout: [ − ] [ Close ] [ + ] across the bottom of the safe region so users
# always have an input-independent way to zoom regardless of whether their
# platform emits scroll events.
MINUS_BUTTON_LOCAL_VERTS = (0.06, 0.91, 0.24, 0.98)
CLOSE_BUTTON_LOCAL_VERTS = (0.32, 0.91, 0.68, 0.98)
PLUS_BUTTON_LOCAL_VERTS = (0.76, 0.91, 0.94, 0.98)
CLOSE_BUTTON_COLOR = (220, 60, 60)
CLOSE_BUTTON_ALPHA = 0.92
ZOOM_BUTTON_COLOR = (60, 110, 220)
ZOOM_BUTTON_ALPHA = 0.92
CLOSE_LABEL_FONT_MULT = 0.7
ZOOM_LABEL_FONT_MULT = 0.9

# Multiplicative zoom step applied each time the +/− button fires.
ZOOM_BUTTON_STEP = 1.35

# Glyph/content zoom is independent from the on-screen panel bounds. The panel
# still stays clamped to the viewport, but the character itself can zoom well
# past the initial 8x size.
GLYPH_MIN_SCALE = 0.05
GLYPH_MAX_SCALE = 16.0

# Minimum uniform scale applied to the safe-region footprint. Upper bound is
# computed dynamically per tick so the panel can grow until *either* axis
# touches the viewport edge (i.e. fills the screen).
#
# The lower bound is intentionally well below ``1 / ZOOM_FONT_SIZE_MULT``
# (where the rendered glyph hits its base font size — i.e. the "character
# edges" — at ``scale = 1/8 = 0.125``) so the user can zoom out past the
# glyph's natural size if they want.
PANEL_MIN_SCALE = 0.05

# Starting size of the panel as a fraction of the safe region. Intentionally
# < 1.0 so the panel has slack against the viewport edges and dragging it
# visibly translates the magnifier window around the screen.
PANEL_DEFAULT_SCALE = 0.7

# Scroll-to-zoom sensitivity by event unit. ``dy`` is scaled by these and then
# subtracted from 1.0 to produce a per-event size multiplier; the result is
# clamped to avoid pathological single-event jumps. Tuned aggressively so a
# few scroll events traverse the full ``[PANEL_MIN_SCALE, scale_max]`` range
# on platforms that emit fine-grained pixel deltas.
SCROLL_STEP_LINE = 0.35
SCROLL_STEP_PIXEL = 0.02
SCROLL_FACTOR_MIN = 0.2
SCROLL_FACTOR_MAX = 5.0


class Magnifier:
    """Per-character hover highlight + click-to-zoom overlay with pan & zoom."""

    def __init__(
        self,
        target,
        base_font_size,
        keyboard=None,
        zoom_font_size_mult=ZOOM_FONT_SIZE_MULT,
    ):
        self.target = target
        self.base_font_size = float(base_font_size)
        self.zoom_font_size = self.base_font_size * float(zoom_font_size_mult)
        self.keyboard = keyboard

        self.zoomed_chars = None
        self.hover_mask = None

        # Panel transform state (refined on first open via ``_reset_panel``).
        self._panel_cx = 0.5
        self._panel_cy = 0.5
        self._panel_scale = PANEL_DEFAULT_SCALE
        self._glyph_scale = 1.0

        # Click / drag state machine.
        self._prev_left_clicking = False
        self._drag_active = False
        self._drag_last_xy = None
        # Set to True when a mouse_down lands on the close button so we don't
        # also start a drag for that same gesture.
        self._suppress_drag_this_press = False

        # Cached current viewport-norm rects, refreshed each tick. ``render``
        # and ``on_events`` read these so the close button stays in sync.
        self._panel_rect = (0.0, 0.0, 1.0, 1.0)
        self._close_rect = (0.0, 0.0, 0.0, 0.0)
        self._minus_rect = (0.0, 0.0, 0.0, 0.0)
        self._plus_rect = (0.0, 0.0, 0.0, 0.0)

        # Widgets are lazy-instantiated on first tick once ``app.safe_region``
        # is available.
        self._zoom_display = None
        self._close_button = None
        self._close_bg = None
        self._close_label = None
        # Zoom +/− are *not* ``xos.ui.button`` widgets — they fire from
        # ``_tick_open`` on the mouse_down rising edge for snappier first-tap
        # response. Only the visuals are real widgets.
        self._minus_bg = None
        self._minus_label = None
        self._plus_bg = None
        self._plus_label = None

    # ------------------------------------------------------------------ state

    def is_open(self):
        return self.zoomed_chars is not None

    def open(self, chars, app=None):
        """Open the zoom overlay with ``chars`` and reset pan/zoom to defaults.

        ``app`` is optional; when supplied, the panel is recentered on the
        current safe-region center so the initial open visually matches the
        previous behavior (panel = safe region).
        """
        self.zoomed_chars = chars
        self._reset_panel(app)
        self._drag_active = False
        self._drag_last_xy = None
        self._suppress_drag_this_press = False
        self._maybe_hide_keyboard()

    def close(self):
        self.zoomed_chars = None
        self._drag_active = False
        self._drag_last_xy = None
        self._suppress_drag_this_press = False

    def _reset_panel(self, app=None):
        self._panel_scale = PANEL_DEFAULT_SCALE
        self._glyph_scale = 1.0
        if app is not None:
            sx1 = float(app.safe_region.x1)
            sy1 = float(app.safe_region.y1)
            sx2 = float(app.safe_region.x2)
            sy2 = float(app.safe_region.y2)
            self._panel_cx = (sx1 + sx2) / 2.0
            self._panel_cy = (sy1 + sy2) / 2.0
        else:
            self._panel_cx = 0.5
            self._panel_cy = 0.5

    def _zoom_in_step(self):
        """Multiplicative zoom-in step driven by the ``+`` button."""
        self._glyph_scale = min(GLYPH_MAX_SCALE, self._glyph_scale * ZOOM_BUTTON_STEP)

    def _zoom_out_step(self):
        """Multiplicative zoom-out step driven by the ``−`` button."""
        self._glyph_scale = max(GLYPH_MIN_SCALE, self._glyph_scale / ZOOM_BUTTON_STEP)

    # ------------------------------------------------------------------ widgets

    def _ensure_widgets(self, app):
        if self._zoom_display is None:
            x1, y1, x2, y2 = app.safe_region.renormalize(0.0, 0.0, 1.0, 1.0)
            self._zoom_display = xos.ui.text(
                "",
                x1=x1,
                y1=y1,
                x2=x2,
                y2=y2,
                font=None,
                color=xos.color.WHITE,
                show_hitboxes=False,
                show_baselines=False,
                editable=False,
                selectable=False,
                scrollable=False,
                show_cursor=False,
                size=self.zoom_font_size,
                alignment=(0.5, 0.5),
                spacing=(1.0, 1.0),
            )
        if self._close_button is None:
            cx1, cy1, cx2, cy2 = app.safe_region.renormalize(*CLOSE_BUTTON_LOCAL_VERTS)
            self._close_bg = xos.ui.rect(
                cx1,
                cy1,
                cx2,
                cy2,
                color=CLOSE_BUTTON_COLOR,
                alpha=CLOSE_BUTTON_ALPHA,
            )
            self._close_button = xos.ui.button(
                cx1,
                cy1,
                cx2,
                cy2,
                on_press=self.close,
            )
            self._close_label = xos.ui.text(
                "Close",
                x1=cx1,
                y1=cy1,
                x2=cx2,
                y2=cy2,
                font=None,
                color=xos.color.WHITE,
                show_hitboxes=False,
                show_baselines=False,
                editable=False,
                selectable=False,
                scrollable=False,
                show_cursor=False,
                size=self.base_font_size * CLOSE_LABEL_FONT_MULT,
                alignment=(0.5, 0.5),
                spacing=(1.0, 1.0),
            )
        # Zoom +/− buttons fire on tick-time ``mouse_down`` rising edges
        # (see ``_tick_open``) so they respond instantly rather than waiting
        # for the standard ``UiButton`` press/release pair — this matters in
        # particular for the first tap right after the magnifier opens, where
        # the OS may have only delivered ``mouse_up`` for the open-gesture.
        if self._minus_bg is None:
            mx1, my1, mx2, my2 = app.safe_region.renormalize(*MINUS_BUTTON_LOCAL_VERTS)
            self._minus_bg = xos.ui.rect(
                mx1, my1, mx2, my2, color=ZOOM_BUTTON_COLOR, alpha=ZOOM_BUTTON_ALPHA,
            )
            self._minus_label = xos.ui.text(
                "−",
                x1=mx1,
                y1=my1,
                x2=mx2,
                y2=my2,
                font=None,
                color=xos.color.WHITE,
                show_hitboxes=False,
                show_baselines=False,
                editable=False,
                selectable=False,
                scrollable=False,
                show_cursor=False,
                size=self.base_font_size * ZOOM_LABEL_FONT_MULT,
                alignment=(0.5, 0.5),
                spacing=(1.0, 1.0),
            )
        if self._plus_bg is None:
            px1b, py1b, px2b, py2b = app.safe_region.renormalize(*PLUS_BUTTON_LOCAL_VERTS)
            self._plus_bg = xos.ui.rect(
                px1b, py1b, px2b, py2b, color=ZOOM_BUTTON_COLOR, alpha=ZOOM_BUTTON_ALPHA,
            )
            self._plus_label = xos.ui.text(
                "+",
                x1=px1b,
                y1=py1b,
                x2=px2b,
                y2=py2b,
                font=None,
                color=xos.color.WHITE,
                show_hitboxes=False,
                show_baselines=False,
                editable=False,
                selectable=False,
                scrollable=False,
                show_cursor=False,
                size=self.base_font_size * ZOOM_LABEL_FONT_MULT,
                alignment=(0.5, 0.5),
                spacing=(1.0, 1.0),
            )

    # ------------------------------------------------------------------ geometry

    def _refresh_panel_rect(self, app):
        """Recompute the clamped panel rect from ``_panel_scale``.

        The panel is anchored to the safe-region width/height at ``scale=1.0``,
        can grow uniformly past the safe region all the way until the smaller
        axis touches the **viewport** edge, and can shrink down to
        ``PANEL_MIN_SCALE`` of the safe-region footprint. The center
        ``_panel_cx/_panel_cy`` is then clamped so the entire panel stays
        inside the viewport ``[0,1]²``.
        """
        sw = max(float(app.safe_region.width), 1e-6)
        sh = max(float(app.safe_region.height), 1e-6)

        # Largest uniform scale before either axis exceeds the viewport.
        # ``max(...)`` here (not ``min(...)``) so the *smaller* safe-region
        # axis is the one that ultimately fills the viewport — the larger
        # axis hits ``1.0`` first and we clamp its panel dimension to 1.0.
        scale_max = max(1.0 / sw, 1.0 / sh)
        scale = max(PANEL_MIN_SCALE, min(scale_max, float(self._panel_scale)))
        self._panel_scale = scale

        # Either axis may saturate at the viewport edge; clamp per-axis.
        pw = min(1.0, sw * scale)
        ph = min(1.0, sh * scale)

        cx_min = pw / 2.0
        cx_max = 1.0 - pw / 2.0
        cy_min = ph / 2.0
        cy_max = 1.0 - ph / 2.0
        # ``cx_min <= cx_max`` always holds because pw <= 1.0.
        cx = min(max(float(self._panel_cx), cx_min), cx_max)
        cy = min(max(float(self._panel_cy), cy_min), cy_max)
        self._panel_cx = cx
        self._panel_cy = cy

        rect = (cx - pw / 2.0, cy - ph / 2.0, cx + pw / 2.0, cy + ph / 2.0)
        self._panel_rect = rect
        return rect

    def _refresh_close_rect(self, app):
        rect = app.safe_region.renormalize(*CLOSE_BUTTON_LOCAL_VERTS)
        self._close_rect = rect
        return rect

    def _refresh_minus_rect(self, app):
        rect = app.safe_region.renormalize(*MINUS_BUTTON_LOCAL_VERTS)
        self._minus_rect = rect
        return rect

    def _refresh_plus_rect(self, app):
        rect = app.safe_region.renormalize(*PLUS_BUTTON_LOCAL_VERTS)
        self._plus_rect = rect
        return rect

    def _point_in_rect_px(self, app, mx, my, rect_norm):
        try:
            fw = float(app.frame["width"])
            fh = float(app.frame["height"])
        except (KeyError, TypeError, ValueError):
            return False
        if fw <= 0.0 or fh <= 0.0:
            return False
        x1, y1, x2, y2 = rect_norm
        return (x1 * fw) <= float(mx) < (x2 * fw) and (y1 * fh) <= float(my) < (y2 * fh)

    def _mouse_xy_norm(self, app):
        try:
            fw = float(app.frame["width"])
            fh = float(app.frame["height"])
            mx = float(app.mouse["x"])
            my = float(app.mouse["y"])
        except (KeyError, TypeError, ValueError):
            return (-1.0, -1.0)
        if fw <= 0.0 or fh <= 0.0:
            return (-1.0, -1.0)
        return (mx / fw, my / fh)

    # ------------------------------------------------------------------ keyboard

    def _maybe_hide_keyboard(self):
        kb = self.keyboard
        if kb is None:
            return
        # ``OnScreenKeyboard.y1`` is 1.0 when fully hidden and <1.0 while
        # animating in or shown.
        y1 = getattr(kb, "y1", 1.0)
        try:
            y1 = float(y1)
        except (TypeError, ValueError):
            y1 = 1.0
        if y1 >= 0.999:
            return
        hide = getattr(kb, "hide", None)
        if callable(hide):
            try:
                hide()
            except (RuntimeError, TypeError):
                pass

    # ------------------------------------------------------------------ tick

    def tick(self, app):
        """Paint the hover highlight and update zoom / drag / close state.

        Must run after ``self.target.tick(app)`` (so the target's
        ``_last_tick_state`` is populated) and before ``self.target.render(app)``
        so the hover highlight sits behind the glyph.
        """
        self._ensure_widgets(app)

        is_clicking = False
        try:
            is_clicking = bool(app.mouse["is_left_clicking"])
        except (KeyError, TypeError):
            is_clicking = False
        just_clicked = is_clicking and not self._prev_left_clicking
        just_released = (not is_clicking) and self._prev_left_clicking

        if self.is_open():
            self._tick_open(app, is_clicking, just_clicked, just_released)
        else:
            self._tick_closed(app, just_clicked)

        self._prev_left_clicking = is_clicking

    def _tick_closed(self, app, just_clicked):
        ts = getattr(self.target, "_last_tick_state", None)
        if ts is None:
            self.hover_mask = None
            return

        hitboxes = ts.hitboxes
        mask = xos.geom.rect.check_point_in_hitboxes(hitboxes, self._mouse_xy_norm(app))
        self.hover_mask = mask

        collided = hitboxes[mask]
        if collided.shape[0] > 0:
            hrect = xos.geom.rect.containing(collided)
            hrect = xos.geom.rect.buffer(hrect, HOVER_BUFFER_SCALE)
            xos.rasterizer.rects_filled(app.frame, hrect, HOVER_HIGHLIGHT_COLOR)

        if just_clicked:
            indices = xos.arange(len(mask))[mask]
            if len(indices) > 0:
                chars = indices.index(str(self.target.text))
                if chars and chars.strip() != "":
                    self.open(chars, app=app)

    def _tick_open(self, app, is_clicking, just_clicked, just_released):
        # Keep the keyboard hidden as long as the overlay is up.
        self._maybe_hide_keyboard()
        self.hover_mask = None

        panel_rect = self._refresh_panel_rect(app)
        close_rect = self._refresh_close_rect(app)
        minus_rect = self._refresh_minus_rect(app)
        plus_rect = self._refresh_plus_rect(app)

        try:
            mx = float(app.mouse["x"])
            my = float(app.mouse["y"])
        except (KeyError, TypeError, ValueError):
            mx = -1.0
            my = -1.0

        in_close = self._point_in_rect_px(app, mx, my, close_rect)
        in_minus = self._point_in_rect_px(app, mx, my, minus_rect)
        in_plus = self._point_in_rect_px(app, mx, my, plus_rect)

        if just_clicked:
            if in_minus:
                # Fire zoom-out immediately on press for instant feedback;
                # don't start a drag.
                self._zoom_out_step()
                self._suppress_drag_this_press = True
                self._drag_active = False
                self._drag_last_xy = None
            elif in_plus:
                self._zoom_in_step()
                self._suppress_drag_this_press = True
                self._drag_active = False
                self._drag_last_xy = None
            elif in_close:
                # Close button still uses release-based ``UiButton`` semantics
                # (handled via on_events) so an accidental touch that slides
                # off can cancel the dismiss.
                self._suppress_drag_this_press = True
                self._drag_active = False
                self._drag_last_xy = None
            elif self._point_in_rect_px(app, mx, my, panel_rect):
                self._suppress_drag_this_press = False
                self._drag_active = True
                self._drag_last_xy = (mx, my)
            else:
                self._suppress_drag_this_press = True
                self._drag_active = False

        if self._drag_active and is_clicking and self._drag_last_xy is not None:
            try:
                fw = float(app.frame["width"])
                fh = float(app.frame["height"])
            except (KeyError, TypeError, ValueError):
                fw = fh = 0.0
            if fw > 0.0 and fh > 0.0:
                last_x, last_y = self._drag_last_xy
                # Drag translates the magnifier window itself (the panel and
                # the glyph centered within it move together). The panel is
                # smaller than the viewport so there's room for it to move;
                # ``_refresh_panel_rect`` re-clamps after each step so the
                # whole panel always stays fully on-screen.
                self._panel_cx += (mx - last_x) / fw
                self._panel_cy += (my - last_y) / fh
                panel_rect = self._refresh_panel_rect(app)
            self._drag_last_xy = (mx, my)

        if just_released or not is_clicking:
            self._drag_active = False
            self._drag_last_xy = None
            if just_released:
                self._suppress_drag_this_press = False

    # ------------------------------------------------------------------ render

    def render(self, app):
        if not self.is_open() or self._zoom_display is None:
            return

        # Recompute rects in case render is called without a preceding tick
        # (defensive; ``StudyApp`` always ticks first).
        panel_rect = self._refresh_panel_rect(app)
        close_rect = self._refresh_close_rect(app)
        minus_rect = self._refresh_minus_rect(app)
        plus_rect = self._refresh_plus_rect(app)

        xos.rasterizer.rects_filled(app.frame, _FULL_FRAME_RECT, ZOOM_BACKDROP_COLOR)

        # The text widget clips drawing to its own ``(x1,y1,x2,y2)`` rect,
        # which must stay in ``[0,1]²``. If we used the small panel rect
        # directly the glyph would get cropped at the panel edge and the
        # surrounding backdrop would bleed through as visible black bands.
        # Instead, render into the *largest* rect that
        #   (a) is centered on the panel center (so drag still translates
        #       the glyph with the cursor), and
        #   (b) fits inside the viewport ``[0,1]²``.
        # That gives the glyph the full available viewport area to draw
        # into, while the (smaller) panel rect remains the drag-clamp
        # region that guarantees the cursor anchor stays on-screen.
        px1, py1, px2, py2 = panel_rect
        panel_cx = (px1 + px2) / 2.0
        panel_cy = (py1 + py2) / 2.0
        half_w = max(1e-3, min(panel_cx, 1.0 - panel_cx))
        half_h = max(1e-3, min(panel_cy, 1.0 - panel_cy))
        glyph_rect_x1 = max(0.0, panel_cx - half_w)
        glyph_rect_y1 = max(0.0, panel_cy - half_h)
        glyph_rect_x2 = min(1.0, panel_cx + half_w)
        glyph_rect_y2 = min(1.0, panel_cy + half_h)

        self._glyph_scale = max(GLYPH_MIN_SCALE, min(GLYPH_MAX_SCALE, self._glyph_scale))
        self._zoom_display.x1 = glyph_rect_x1
        self._zoom_display.y1 = glyph_rect_y1
        self._zoom_display.x2 = glyph_rect_x2
        self._zoom_display.y2 = glyph_rect_y2
        self._zoom_display.size = self.zoom_font_size * self._glyph_scale
        self._zoom_display.text = self.zoomed_chars
        self._zoom_display.tick(app)
        self._zoom_display.render(app)

        # Sync bottom-row button widget verts each frame so safe-region
        # changes (e.g. on rotation) follow without rebuilding the widgets.
        def _paint_button(bg, btn, label, rect):
            x1, y1, x2, y2 = rect
            if bg is not None:
                bg.verts = (x1, y1, x2, y2)
                bg.render(app)
            if btn is not None:
                btn.verts = (x1, y1, x2, y2)
            if label is not None:
                label.x1 = x1
                label.y1 = y1
                label.x2 = x2
                label.y2 = y2
                label.tick(app)
                label.render(app)

        # +/− are visual-only (no ``UiButton``); the click is handled in
        # ``_tick_open`` on the mouse_down rising edge.
        _paint_button(self._minus_bg, None, self._minus_label, minus_rect)
        _paint_button(self._close_bg, self._close_button, self._close_label, close_rect)
        _paint_button(self._plus_bg, None, self._plus_label, plus_rect)

    # ------------------------------------------------------------------ events

    def on_events(self, app):
        if not self.is_open():
            return

        # Only the Close button uses ``UiButton`` press/release semantics;
        # +/− fire on the mouse_down rising edge in ``_tick_open`` instead.
        if self._close_button is not None:
            try:
                self._close_button.on_events(app)
            except RuntimeError:
                pass

        # Don't propagate input to the zoom-display widget — it's display-only
        # and forwarding events made it eat scroll wheel deltas.
        ev = getattr(app, "_xos_event", None)
        if not isinstance(ev, dict):
            return
        if ev.get("kind") != "scroll":
            return
        try:
            dy = float(ev.get("dy", 0.0))
        except (TypeError, ValueError):
            return
        unit = str(ev.get("unit", "line"))
        step = SCROLL_STEP_LINE if unit == "line" else SCROLL_STEP_PIXEL
        # Positive dy = scroll down = zoom out; negative dy = scroll up = zoom in.
        factor = 1.0 - dy * step
        factor = max(SCROLL_FACTOR_MIN, min(SCROLL_FACTOR_MAX, factor))
        self._glyph_scale = max(
            GLYPH_MIN_SCALE,
            min(GLYPH_MAX_SCALE, self._glyph_scale * factor),
        )
        # Re-clamp the panel boundary too; glyph zoom is independent, but the
        # magnifier box itself must remain fully on-screen.
        self._refresh_panel_rect(app)
