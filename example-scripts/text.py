import xos


class TextDemo(xos.Application):
    def setup(self):
        self.text = xos.ui.text(
            "hello world",
            0, 0,  # top left
            1.0, 1.0,  # bottom right (normalized viweport coordinates)
            color=xos.color.WHITE,
            hitboxes=True,
            # baselines=True,  # TODO: add baselines rendering
        )


    def tick(self):
        self.frame.clear(xos.color.BLACK)
        self.text.render(self.frame)

    # TODO: general event handling
    # def on_events(self, state: xos.EngineState):
    #     self.text.on_events(
    #         state,
    #         scrolling=True,
    #         clicking=True,
    #         typing=True,
    #         selecting=True,
    #         pasting=True,
    #         copying=True,
    #         # shortcuts=True,  # TODO later we can add shortcuts
    #     )


if __name__ == "__main__":
    TextDemo().run()

