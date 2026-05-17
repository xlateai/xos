import xos


def make_whiteboard(self, x1=0.0, y1=0.0, x2=1.0, y2=1.0):
    # x1, y1, x2, y2 = self.safe_region.renormalize(x1, y1, x2, y2)
    return xos.ui.whiteboard(
        x1=x1,
        y1=y1,
        x2=x2,
        y2=y2,
        editable=True,
        color=xos.color.WHITE,
        thickness=2.0,
        # scrollable_x=True,  # default true, allows infinite movement around the board
        # scrollable_y=True,  # default true, allows infinite movement around the board
        # zoomable=True,  # default true, allows zooming in and out of the board
    )


class WhiteboardDemo(xos.Application):
    def __init__(self):
        super().__init__()

        self.whiteboard = make_whiteboard(self)

    def tick(self):
        self.frame.clear(xos.color.BLACK)
        self.whiteboard.tick(self)
        self.whiteboard.render(self)

        if self.t % 300 == 0:
            print("fps:", self.fps)

    def on_events(self):
        self.whiteboard.on_events(self)


if __name__ == "__main__":
    WhiteboardDemo().run()
