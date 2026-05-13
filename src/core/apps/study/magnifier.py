"""Click-to-zoom magnifier overlay for a target ``xos.ui.Text`` widget.

Visually identical to the inline zoom code that used to live in ``study.py``:

  * Hovering the cursor over a glyph draws a translucent highlight behind the
    collided hitbox(es), using the same ``xos.rasterizer.rects_filled`` /
    ``xos.geom.rect.containing`` + ``buffer`` raster pipeline as the lime
    backdrop in ``StudyApp.tick``.
  * Clicking on a hovered glyph opens a full-frame zoomed-in view of the
    selected character(s) on top of a darkened backdrop. Clicking again
    dismisses the overlay.

Owns the per-character hit-testing, click edge-detection, and the oversized
``xos.ui.Text`` widget that paints the zoomed glyph. Designed to be a drop-in
component the app composes alongside ``menu.Menu``: ``tick(app)`` paints the
hover highlight (before the target's ``render``) and ``render(app)`` paints the
zoomed overlay (after the target's ``render``).

Extension points (kept intentionally minimal for now):

  * ``Magnifier.zoomed_chars`` — current zoom selection, ``None`` when closed.
  * Subclasses / callers can read ``Magnifier.hover_mask`` after ``tick`` to
    inspect which hitboxes were under the cursor this frame.
"""

import xos


HOVER_HIGHLIGHT_COLOR = (*xos.color.CYAN, 0.8)
ZOOM_BACKDROP_COLOR = (0, 0, 0, 0.88)
ZOOM_FONT_SIZE_MULT = 8.0
HOVER_BUFFER_SCALE = 1.2

# Cached so we don't reallocate the same constant rect tensor every tick.
_FULL_FRAME_RECT = xos.tensor([0.0, 0.0, 1.0, 1.0], shape=(2, 2))


class Magnifier:
    """Per-character hover highlight + click-to-zoom overlay over ``target``.

    ``target`` is an ``xos.ui.Text`` whose ``_last_tick_state`` provides the
    hitbox / character-index tensors after ``target.tick(app)`` has run this
    frame. ``base_font_size`` is the source size used to scale the zoomed glyph
    via ``zoom_font_size_mult``.
    """

    def __init__(self, target, base_font_size, zoom_font_size_mult=ZOOM_FONT_SIZE_MULT):
        self.target = target
        self.base_font_size = float(base_font_size)
        self.zoom_font_size = self.base_font_size * float(zoom_font_size_mult)

        self.zoomed_chars = None
        self.hover_mask = None

        self._prev_left_clicking = False
        # Lazy-instantiated on first tick so we can pull the live safe region from app.
        self._zoom_display = None

    def _ensure_zoom_display(self, app):
        if self._zoom_display is not None:
            return
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

    def _click_rising_edge(self, app):
        try:
            cur = bool(app.mouse["is_left_clicking"])
        except (KeyError, TypeError):
            cur = False
        rising = cur and not self._prev_left_clicking
        self._prev_left_clicking = cur
        return rising

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

    def is_open(self):
        return self.zoomed_chars is not None

    def close(self):
        self.zoomed_chars = None

    def tick(self, app):
        """Paint the hover highlight and update zoom state.

        Must be called after ``self.target.tick(app)`` (so the target's
        ``_last_tick_state`` is populated) and before ``self.target.render(app)``
        so the highlight sits behind the glyph.
        """
        self._ensure_zoom_display(app)
        ts = getattr(self.target, "_last_tick_state", None)
        if ts is None:
            self.hover_mask = None
            return

        hitboxes = ts.hitboxes
        mask = xos.geom.rect.check_point_in_hitboxes(hitboxes, self._mouse_xy_norm(app))
        self.hover_mask = mask

        if self.zoomed_chars is None:
            collided = hitboxes[mask]
            if collided.shape[0] > 0:
                hrect = xos.geom.rect.containing(collided)
                hrect = xos.geom.rect.buffer(hrect, HOVER_BUFFER_SCALE)
                xos.rasterizer.rects_filled(app.frame, hrect, HOVER_HIGHLIGHT_COLOR)

        if self._click_rising_edge(app):
            if self.zoomed_chars is not None:
                self.zoomed_chars = None
            else:
                indices = xos.arange(len(mask))[mask]
                if len(indices) > 0:
                    chars = indices.index(str(self.target.text))
                    if chars and chars.strip() != "":
                        self.zoomed_chars = chars

    def render(self, app):
        """Paint the zoomed overlay (if open). Call after the target's ``render``."""
        if self.zoomed_chars is None or self._zoom_display is None:
            return
        xos.rasterizer.rects_filled(app.frame, _FULL_FRAME_RECT, ZOOM_BACKDROP_COLOR)
        self._zoom_display.text = self.zoomed_chars
        self._zoom_display.tick(app)
        self._zoom_display.render(app)

    def on_events(self, app):
        if self._zoom_display is not None:
            self._zoom_display.on_events(app)
