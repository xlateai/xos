import xos

import study_data
import menu

DEFAULT_FONT_SIZE = 52.0

HOVER_HIGHLIGHT_COLOR = (*xos.color.CYAN, 0.45)
ZOOM_BACKDROP_COLOR = (0, 0, 0, 0.88)
ZOOM_FONT_SIZE = DEFAULT_FONT_SIZE * 8.0


def _boxed_text(app, text, x1, y1, x2, y2, **kwargs):
    x1, y1, x2, y2 = app.safe_region.renormalize(x1, y1, x2, y2)
    base = dict(
        font=None,
        color=xos.color.WHITE,
        show_hitboxes=False,
        show_baselines=False,
    )
    base.update(kwargs)
    return xos.ui.text(text, x1=x1, y1=y1, x2=x2, y2=y2, **base)


def _draw_norm_rect(app, x1, y1, x2, y2, color):
    rect = xos.tensor([float(x1), float(y1), float(x2), float(y2)], shape=(2, 2))
    xos.rasterizer.rects_filled(app.frame, rect, color)


class TextDemo(xos.Application):
    def __init__(self):
        super().__init__()

        self.data = study_data.StudyData()
        self._awaiting_guess = True

        self.keyboard = xos.ui.onscreen_keyboard()

        self.vocab_display = _boxed_text(
            self,
            "…",
            0.0,
            0.0,
            1.0,
            0.26,
            editable=False,
            selectable=True,
            scrollable=True,
            show_cursor=False,
            show_hitboxes=False,
            size=DEFAULT_FONT_SIZE * 1.85,
            alignment=(0.5, 1.0),
            spacing=(1.5, 1.45),
        )

        self.guess_area = _boxed_text(
            self,
            "",
            0.0,
            0.26,
            1.0,
            0.40,
            editable=True,
            selectable=True,
            scrollable=False,
            show_cursor=True,
            size=DEFAULT_FONT_SIZE,
            alignment=(0.5, 0.5),
            spacing=(1.0, 1.05),
            shortcuts=True,
        )

        self.description = _boxed_text(
            self,
            self.data.prompt_markup(),
            0.0,
            0.40,
            1.0,
            1.0,
            editable=False,
            selectable=True,
            scrollable=True,
            show_cursor=False,
            size=DEFAULT_FONT_SIZE * 0.88,
            alignment=(0.5, 0.0),
            spacing=(1.0, 1.45),
        )

        self.text = xos.ui.group(self.vocab_display, self.guess_area, self.description)
        self._bootstrap_round()

        self.menu = menu.Menu()

        # Per-character hover + click-to-zoom state for the vocab headline.
        self._hover_box_norm = None
        self._hover_char = None
        self._prev_left_clicking = False
        self._zoomed_char = None

        # Full-frame oversized text used to render the zoomed-in character.
        self._zoom_display = _boxed_text(
            self,
            "",
            0.0,
            0.0,
            1.0,
            1.0,
            editable=False,
            selectable=False,
            scrollable=False,
            show_cursor=False,
            size=ZOOM_FONT_SIZE,
            alignment=(0.5, 0.5),
            spacing=(1.0, 1.0),
        )

    def _headline(self, row):
        if not row:
            return "?"
        w = str(row.get("Vocab-expression", "") or "").strip()
        return w or str(row.get("Vocab-kana", "") or "").strip() or "?"

    def _bootstrap_round(self):
        row = self.data.next_example()
        self.vocab_display.text = self._headline(row)
        self._awaiting_guess = True
        self.description.text = self.data.prompt_markup()

    def _clear_guess_native(self):
        nid = getattr(self.guess_area, "_native_id", None)
        if nid is not None:
            try:
                xos.ui._text_set_document(int(nid), "", False)
            except (ValueError, RuntimeError, OSError):
                pass
        self.guess_area.text = ""

    def _submit_guess(self, raw: str):
        line = str(raw).strip()
        empty = line == ""
        ok, msg = self.data.check_answer(line)
        self.description.text = (
            self.data.breakdown_markup(ok, empty_guess=empty)
            + "\n\n["
            + msg
            + "](color=GRAY size=30)"
        )
        self._awaiting_guess = False
        self._clear_guess_native()

    def _update_vocab_hover(self):
        # Hover detection is paused while the zoom overlay is showing.
        if self._zoomed_char is not None:
            self._hover_box_norm = None
            self._hover_char = None
            return

        ts = getattr(self.vocab_display, "_last_tick_state", None)
        if ts is None:
            self._hover_box_norm = None
            self._hover_char = None
            return

        try:
            flat = ts.hitboxes._data["_data"]
            idx_flat = ts.hitbox_char_indices._data["_data"]
        except (KeyError, AttributeError, TypeError):
            self._hover_box_norm = None
            self._hover_char = None
            return

        n = len(flat) // 4
        if n == 0:
            self._hover_box_norm = None
            self._hover_char = None
            return

        try:
            fw = float(self.frame["width"])
            fh = float(self.frame["height"])
            mx = float(self.mouse["x"])
            my = float(self.mouse["y"])
        except (KeyError, TypeError, ValueError):
            return
        if fw <= 0.0 or fh <= 0.0:
            return

        text = str(self.vocab_display.text)
        for i in range(n):
            x1n = float(flat[4 * i])
            y1n = float(flat[4 * i + 1])
            x2n = float(flat[4 * i + 2])
            y2n = float(flat[4 * i + 3])
            xa, xb = (x1n, x2n) if x1n <= x2n else (x2n, x1n)
            ya, yb = (y1n, y2n) if y1n <= y2n else (y2n, y1n)
            if xa * fw <= mx < xb * fw and ya * fh <= my < yb * fh:
                try:
                    visual_idx = int(idx_flat[i]) if i < len(idx_flat) else i
                except (ValueError, TypeError):
                    visual_idx = i
                ch = text[visual_idx] if 0 <= visual_idx < len(text) else None
                # Whitespace hitboxes shouldn't trigger zoom; still highlight tangible chars.
                if ch is not None and ch.strip() == "":
                    ch = None
                self._hover_box_norm = (xa, ya, xb, yb)
                self._hover_char = ch
                return

        self._hover_box_norm = None
        self._hover_char = None

    def _click_rising_edge(self):
        try:
            cur = bool(self.mouse["is_left_clicking"])
        except (KeyError, TypeError):
            cur = False
        rising = cur and not self._prev_left_clicking
        self._prev_left_clicking = cur
        return rising

    def tick(self):
        # Sticky input focus: keep accepting keys even when tapping scrollable panes.
        self.guess_area.focused = True

        self.keyboard.tick(self)
        self.frame.clear(xos.color.BLACK)

        ts_vocab, _, _ = self.text.tick(self)
        self.description.y2 = self.keyboard.y1

        vocab_rect = xos.geom.rect.containing(ts_vocab.hitboxes)
        vocab_rect = xos.geom.rect.buffer(vocab_rect, 1.2)
        xos.rasterizer.rects_filled(self.frame, vocab_rect, (*xos.color.LIME, 0.78))

        self._update_vocab_hover()
        if self._click_rising_edge():
            if self._zoomed_char is not None:
                self._zoomed_char = None
            elif self._hover_char is not None:
                self._zoomed_char = self._hover_char
                # Clear hover state so the highlight disappears while zoomed.
                self._hover_box_norm = None
                self._hover_char = None

        # Drawn before text.render() so the highlight sits behind the glyph.
        if self._hover_box_norm is not None and self._zoomed_char is None:
            x1n, y1n, x2n, y2n = self._hover_box_norm
            _draw_norm_rect(self, x1n, y1n, x2n, y2n, HOVER_HIGHLIGHT_COLOR)

        raw = getattr(self.guess_area, "text", "")
        if "\n" in raw or "\r" in raw:
            line = _first_line(raw).strip()
            if self._awaiting_guess:
                # Enter always submits and shows the breakdown + reading (empty = reveal only).
                self._submit_guess(line)
            else:
                self._bootstrap_round()
                self._clear_guess_native()

        self.text.render(self)

        if self._zoomed_char is not None:
            _draw_norm_rect(self, 0.0, 0.0, 1.0, 1.0, ZOOM_BACKDROP_COLOR)
            self._zoom_display.text = self._zoomed_char
            self._zoom_display.tick(self)
            self._zoom_display.render(self)

        # TODO somehow make the tick render thing better? its decent though.
        self.menu.tick(self)
        self.menu.render(self)

    def on_events(self):
        self.text.on_events(self)
        self.keyboard.on_events(self)
        self.menu.on_events(self)


def _first_line(s: str) -> str:
    s = str(s).replace("\r\n", "\n").replace("\r", "\n")
    return s.split("\n", 1)[0]


if __name__ == "__main__":
    TextDemo().run()
