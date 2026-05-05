import xos


import xos

# Canonical `xos app text` / iOS app name `text` — lives beside the native TextApp Rust module.

DEFAULT_FONT_SIZE = 48.0


def make_text(self, x1=0.0, y1=0.0, x2=1.0, y2=1.0, text: str = "", fontsize: float=1.0, alignment=(0.0, 0.0), spacing=(1.0, 1.0)):
    x1, y1, x2, y2 = self.safe_region.renormalize(x1, y1, x2, y2)
    return xos.ui.text(
        text,
        x1=x1,
        y1=y1,
        x2=x2,
        y2=y2,
        editable=False,
        font=None,
        font_size=DEFAULT_FONT_SIZE * fontsize,
        color=xos.color.WHITE,
        show_hitboxes=False,
        show_baselines=False,
        selectable=True,
        scrollable=True,
        show_cursor=False,
        alignment=alignment,
        spacing=spacing,
    )


class TextDemo(xos.Application):
    def __init__(self):
        super().__init__()

        self.keyboard = xos.ui.onscreen_keyboard()

        self.vocab_display = make_text(self, x1=0.0, y1=0.0, x2=1.0, y2=0.2, text="図書館", fontsize=1.8, alignment=(0.5, 0.5), spacing=(1.5, 1.5))
        self.description = make_text(self, x1=0.0, y1=0.2, x2=1.0, y2=1.0, text="toshokann (library)", fontsize=1.0, alignment=(0.5, 0.0))
        self.text = xos.ui.group(self.vocab_display, self.description)

    def tick(self):
        # self.text.font_size = DEFAULT_FONT_SIZE * self.scale

        self.keyboard.tick(self)
        self.frame.clear(xos.color.BLACK)
        # ts = self.text.tick(self)

        ts = self.text.tick(self)[0]

        # self.vocab_display.y2 = self.keyboard.y1
        self.description.y2 = self.keyboard.y1

        vocab_rect = xos.geom.rect.containing(ts.hitboxes)  # write the min and max reduction of ts.hitboxes into this singular rect. should return shape (2, 2) for the containing rectangle (the rect that contains all the input rectangles/hitboxes).
        # rasterize the rectangle into the frame over the text but as a slight highlight/transparent overlay
        # also, add a buffer so that the rectangle is slightly larger
        vocab_rect = xos.geom.rect.buffer(vocab_rect, 1.2)  # gives a 1.2x multiply on the area of the rectangel(s) supports (k, 2, 2) or (2, 2) for batch and non-batch
        xos.rasterizer.rects_filled(self.frame, vocab_rect, color=(*xos.color.LIME, 0.25))

        if self.t % 300 == 0:
            print("fps:", self.fps)
            print("lines:", ts.lines.shape)
            print("hitboxes:", ts.hitboxes.shape)
            print("baselines:", ts.baselines.shape)

    def on_events(self):
        self.keyboard.on_events(self)
        self.text.on_events(self)


if __name__ == "__main__":
    TextDemo().run()
