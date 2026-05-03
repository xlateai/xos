import xos


DEFAULT_FONT_SIZE = 48.0


class TextDemo(xos.Application):
    def __init__(self):
        super().__init__()

        self.keyboard = xos.ui.onscreen_keyboard()
        # Font uses the engine/F3 default family; `scale` applies on each tick () after xos_sync.
        self.text = xos.ui.text(
            "",
            0,
            0,  # top left
            1.0,
            1.0,  # bottom right (normalized viewport coordinates)
            editable=True,
            font=None,
            font_size=DEFAULT_FONT_SIZE,
            color=xos.color.WHITE,
            hitboxes=True,
            baselines=True,
            selectable=True,
            scrollable=True,
            show_cursor=True,
        )

    def tick(self):
        # Matches F3 UI scale slider (percent/100) each frame — synced before your tick runs.
        self.text.font_size = DEFAULT_FONT_SIZE * self.scale

        self.keyboard.tick(self)
        self.frame.clear(xos.color.BLACK)
        ts = self.text.tick(self)

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

