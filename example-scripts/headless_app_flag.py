import xos

class HeadlessFlagExample(xos.Application):
    # A "Headless" application is an application that
    # *does* have a viewport, but it doesn't get rendered.
    headless: bool = True

    def __init__(self):
        super().__init__()

    def tick(self):
        self.frame.clear(xos.color.BLACK)
        print(self.frame.tensor.shape, f"fps: {self.fps:.2f}", f"tick: {self.t}")


if __name__ == "__main__":
    hfe = HeadlessFlagExample()
    hfe.run()