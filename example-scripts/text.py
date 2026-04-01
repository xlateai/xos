import xos


class TextDemo(xos.Application):
    def setup(self):
        self.text = xos.ui.text(
            "hello world",
            0, 0,  # top left
            1.0, 1.0,  # bottom right (normalized viweport coordinates)
            color=xos.color.WHITE,
            hitboxes=True,
        )


    def tick(self):
        self.frame.clear(xos.color.BLACK)
        self.text.render(self.frame)


if __name__ == "__main__":
    TextDemo().run()

