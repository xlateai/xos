import xos


class TVApp(xos.Application):
    def __init__(self):
        super().__init__()

        # frame is initialized with random static
        self.randomize_frame()
        self.randomize_kernel()

    def randomize_frame(self):
        xos.random.uniform_fill(self.frame.tensor, 0.0, 255.0)
    
    def randomize_kernel(self):
        self.kernel = xos.random.uniform(0.001, 1.001, shape=(3, 3, 3), dtype=xos.float32)

    def tick(self):
        # convolution tv will convolve the random frame
        xos.ops.convolve(self.frame.tensor, self.kernel, inplace=True)

    def on_screen_size_change(self, width, height):
        self.randomize_frame()
        self.randomize_kernel()
        print(width, height)

    def on_events(self):
        pass


if __name__ == "__main__":
    TVApp().run()