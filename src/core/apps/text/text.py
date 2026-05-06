import xos

# Canonical `xos app text` / iOS app name `text` — lives beside the native TextApp Rust module.

DEFAULT_FONT_SIZE = 48.0


def make_text(self, x1=0.0, y1=0.0, x2=1.0, y2=1.0):
    x1, y1, x2, y2 = self.safe_region.renormalize(x1, y1, x2, y2)
    return xos.ui.text(
        "",
        x1=x1,
        y1=y1,
        x2=x2,
        y2=y2,
        editable=True,
        font=None,
        size=DEFAULT_FONT_SIZE,
        color=xos.color.WHITE,
        hitboxes=True,
        baselines=True,
        selectable=True,
        scrollable=True,
        show_cursor=True,
    )


class TextDemo(xos.Application):
    def __init__(self):
        super().__init__()

        self.keyboard = xos.ui.onscreen_keyboard()

        self.text1 = make_text(self, x1=0.0, y1=0.0, x2=0.5, y2=1.0)
        self.text2 = make_text(self, x1=0.5, y1=0.0, x2=1.0, y2=1.0)
        self.text = xos.ui.group(self.text1, self.text2)

    def tick(self):
        self.text.size = DEFAULT_FONT_SIZE * self.scale

        self.keyboard.tick(self)
        self.frame.clear(xos.color.BLACK)
        # `xos.ui.group(...).tick()` returns one state per child widget.
        ts = self.text.tick(self)[0]

        self.text1.y2 = self.keyboard.y1
        self.text2.y2 = self.keyboard.y1

        self.text.render(self)

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
