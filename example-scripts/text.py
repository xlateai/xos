import xos


class TextDemo(xos.Application):
    def setup(self):
        self.text = xos.ui.text(
            "hello world",
            0, 0,  # top left
            1.0, 1.0,  # bottom right (normalized viweport coordinates)
            color=xos.color.WHITE,
            hitboxes=True,
            baselines=False,
        )


    def tick(self):
        # color = xos.color.WHITE if self.t % 2 == 0 else xos.color.RED

        self.frame.clear(xos.color.BLACK)

        text_state = self.text.render(
            self.frame,
            # color=xos.color.WHITE,
            # hitboxes=True,
            # baselines=True,
        )

        print(self.fps)
        print(text_state.lines.shape)
        print(text_state.hitboxes.shape)
        print(text_state.baselines.shape)

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

