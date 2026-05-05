import xos


class TextDemo(xos.Application):
    def __init__(self):
        super().__init__()

        self.keyboard = xos.ui.onscreen_keyboard()
        self.text = xos.ui.text(
            "hello world",
            0, 0,  # top left
            1.0, 1.0,  # bottom right (normalized viweport coordinates)
            font=None,  # default font
            size=24.0 * self.scale,  # scaling text size with F3 menu
            color=xos.color.WHITE,
            hitboxes=True,
            baselines=True,
            selectable=True,
            scrollable=True,
            editable=True,
            show_cursor=True,
        )

    def tick(self):
        # color = xos.color.WHITE if self.t % 2 == 0 else xos.color.RED

        self.keyboard.tick(self)
        self.frame.clear(xos.color.BLACK)

        # ts = self.text.render(
        #     self.frame,
        # )
        ts = self.text.tick(self)

        if self.t % 300 == 0:
            print("fps:", self.fps)
            print("lines:", ts.lines.shape)
            print("hitboxes:", ts.hitboxes.shape)
            print("baselines:", ts.baselines.shape)

    def on_events(self):
        self.text.on_events(self)
        self.keyboard.on_events(self)

if __name__ == "__main__":
    TextDemo().run()

