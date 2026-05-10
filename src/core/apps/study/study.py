import xos

import study_data
import menu

DEFAULT_FONT_SIZE = 52.0


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
