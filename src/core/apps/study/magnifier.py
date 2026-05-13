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
      - **Pan** the panel by left-click-dragging anywhere inside it.
      - **Zoom** the panel via scroll wheel / trackpad (positive dy = zoom out,
        negative dy = zoom in). The panel size is clamped to ``[0.15, 1.0]``
        of the safe region in each axis.
      - **Dismiss** the overlay via a dedicated Close button pinned to the
        bottom-center of the safe region (clicks elsewhere never dismiss).
  * The panel's bounding box is always renormalized + clamped to the safe
    region every tick, so it cannot leave the screen no matter what
    transforms (pan or zoom) have been applied.
  * If a host-managed onscreen keyboard reference is supplied via the
    ``keyboard`` constructor argument, it is hidden whenever the magnifier
    opens (and again on any tick while open, so it stays hidden if something
    else re-shows it).

Coordinate conventions:

  * ``_panel_cx, _panel_cy`` — panel center in normalized viewport coords
    (same space as ``xos.ui.Text.x1`` etc.).
  * ``_panel_size_norm`` — panel size as a fraction of the safe region
    (1.0 = panel fills the entire safe region).
"""

import xos


HOVER_HIGHLIGHT_COLOR = (*xos.color.CYAN, 0.8)
ZOOM_BACKDROP_COLOR = (0, 0, 0, 0.88)
ZOOM_FONT_SIZE_MULT = 8.0
HOVER_BUFFER_SCALE = 1.2

# Cached so we don't reallocate the same constant rect tensor every tick.
_FULL_FRAME_RECT = xos.tensor([0.0, 0.0, 1.0, 1.0], shape=(2, 2))

# Close button geometry expressed in safe-region-local coords (renormalized
# onto the viewport every tick via ``app.safe_region.renormalize``).
CLOSE_BUTTON_LOCAL_VERTS = (0.32, 0.91, 0.68, 0.98)
CLOSE_BUTTON_COLOR = (220, 60, 60)
CLOSE_BUTTON_ALPHA = 0.92
CLOSE_LABEL_FONT_MULT = 0.7

# Panel size clamps (fraction of safe region per axis).
PANEL_MIN_SIZE = 0.15
PANEL_MAX_SIZE = 1.0

# Scroll-to-zoom sensitivity by event unit. ``dy`` is scaled by these and then
# subtracted from 1.0 to produce a per-event size multiplier; the result is
# clamped to avoid pathological single-event jumps.
SCROLL_STEP_LINE = 0.08
SCROLL_STEP_PIXEL = 0.004
SCROLL_FACTOR_MIN = 0.5
SCROLL_FACTOR_MAX = 2.0


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

        # Panel transform state (set on first open via ``_reset_panel``).
        self._panel_cx = 0.5
        self._panel_cy = 0.5
        self._panel_size_norm = PANEL_MAX_SIZE

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

        # Widgets are lazy-instantiated on first tick once ``app.safe_region``
        # is available.
        self._zoom_display = None
        self._close_button = None
        self._close_bg = None
        self._close_label = None

    # ------------------------------------------------------------------ state

    def is_open(self):
        return self.zoomed_chars is not None

    def open(self, chars):
        """Open the zoom overlay with ``chars`` and reset pan/zoom to defaults."""
        self.zoomed_chars = chars
        self._reset_panel()
        self._drag_active = False
        self._drag_last_xy = None
        self._suppress_drag_this_press = False
        self._maybe_hide_keyboard()

    def close(self):
        self.zoomed_chars = None
        self._drag_active = False
        self._drag_last_xy = None
        self._suppress_drag_this_press = False

    def _reset_panel(self):
        self._panel_cx = 0.5
        self._panel_cy = 0.5
        self._panel_size_norm = PANEL_MAX_SIZE

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

    # ------------------------------------------------------------------ geometry

    def _refresh_panel_rect(self, app):
        """Recompute and store the clamped panel rect, then mirror it onto
        ``_panel_cx/_panel_cy`` so subsequent ticks start from a valid pose.
        """
        sx1 = float(app.safe_region.x1)
        sy1 = float(app.safe_region.y1)
        sx2 = float(app.safe_region.x2)
        sy2 = float(app.safe_region.y2)
        sw = max(sx2 - sx1, 1e-6)
        sh = max(sy2 - sy1, 1e-6)

        size = max(PANEL_MIN_SIZE, min(PANEL_MAX_SIZE, float(self._panel_size_norm)))
        self._panel_size_norm = size

        pw = sw * size
        ph = sh * size
        cx_min = sx1 + pw / 2.0
        cx_max = sx2 - pw / 2.0
        cy_min = sy1 + ph / 2.0
        cy_max = sy2 - ph / 2.0
        # ``cx_min <= cx_max`` always holds because pw <= sw.
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
                    self.open(chars)

    def _tick_open(self, app, is_clicking, just_clicked, just_released):
        # Keep the keyboard hidden as long as the overlay is up.
        self._maybe_hide_keyboard()
        self.hover_mask = None

        panel_rect = self._refresh_panel_rect(app)
        close_rect = self._refresh_close_rect(app)

        mx = float(app.mouse["x"]) if "x" in app.mouse else -1.0
        my = float(app.mouse["y"]) if "y" in app.mouse else -1.0

        if just_clicked:
            if self._point_in_rect_px(app, mx, my, close_rect):
                # The close button consumes this gesture; ``UiButton``
                # already handles the press/release pair via on_events.
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
                self._panel_cx += (mx - last_x) / fw
                self._panel_cy += (my - last_y) / fh
                # Re-clamp so the panel stays fully on-screen.
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

        xos.rasterizer.rects_filled(app.frame, _FULL_FRAME_RECT, ZOOM_BACKDROP_COLOR)

        px1, py1, px2, py2 = panel_rect
        sx1 = float(app.safe_region.x1)
        sy1 = float(app.safe_region.y1)
        sx2 = float(app.safe_region.x2)
        sy2 = float(app.safe_region.y2)
        sw = max(sx2 - sx1, 1e-6)
        sh = max(sy2 - sy1, 1e-6)
        # Use the smaller axis ratio so the glyph never overflows the panel.
        axis_ratio = min((px2 - px1) / sw, (py2 - py1) / sh)
        self._zoom_display.x1 = px1
        self._zoom_display.y1 = py1
        self._zoom_display.x2 = px2
        self._zoom_display.y2 = py2
        self._zoom_display.size = self.zoom_font_size * max(axis_ratio, 0.05)
        self._zoom_display.text = self.zoomed_chars
        self._zoom_display.tick(app)
        self._zoom_display.render(app)

        # Sync close button widget verts each frame so safe-region changes
        # (e.g. on rotation) follow without rebuilding the widgets.
        cx1, cy1, cx2, cy2 = close_rect
        if self._close_bg is not None:
            self._close_bg.verts = (cx1, cy1, cx2, cy2)
            self._close_bg.render(app)
        if self._close_button is not None:
            self._close_button.verts = (cx1, cy1, cx2, cy2)
        if self._close_label is not None:
            self._close_label.x1 = cx1
            self._close_label.y1 = cy1
            self._close_label.x2 = cx2
            self._close_label.y2 = cy2
            self._close_label.tick(app)
            self._close_label.render(app)

    # ------------------------------------------------------------------ events

    def on_events(self, app):
        if not self.is_open():
            return

        # Forward to the close button so it can pair mouse_down + mouse_up
        # within its own rect (the standard ``UiButton`` press semantics).
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
        new_size = self._panel_size_norm * factor
        self._panel_size_norm = max(PANEL_MIN_SIZE, min(PANEL_MAX_SIZE, new_size))
        # Re-clamp center now that the size changed.
        self._refresh_panel_rect(app)
