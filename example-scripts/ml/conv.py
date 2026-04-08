"""
Convolution demo — one random 3x3x3 kernel; each reset fills the frame with
fresh random RGBA noise (startup and on window resize).
"""
import xos


class Convolution(xos.Application):
    def __init__(self):
        super().__init__()
        self.kernel = None
        self.needs_init = True
        xos.print("Convolution Demo initialized")
        kernel_size = 3
        self.stride = 1
        self.kernel = xos.random.uniform(0.001, 1.001, shape=(kernel_size, kernel_size, 3), dtype=xos.float32)
        xos.print(f"Generated random {kernel_size}x{kernel_size}x3 kernel")
        xos.print("Setup complete!")
    
    def reset_state(self):
        """Fill the frame with a new random RGBA image (same size as the viewport)."""
        width = self.get_width()
        height = self.get_height()
        noise = xos.random.uniform(
            0.0, 255.0, shape=(height, width, 4), dtype=xos.uint8
        )
        self.frame.tensor[:] = noise
        xos.print(f"Random {width}x{height} RGBA image")
    
    def tick(self):
        """First tick: init image. Then: apply convolution every frame."""
        if self.needs_init:
            self.reset_state()
            xos.print("Starting convolution...")
            self.needs_init = False
            return

        xos.ops.convolve(self.frame.tensor, self.kernel, stride=self.stride, inplace=True)
        # self.frame.tensor[:] = result
    
    def on_screen_size_change(self, width, height):
        xos.print(f"Screen resized to {width}x{height}")
        self.reset_state()


if __name__ == "__main__":
    xos.print("Convolution Demo")
    app = Convolution()
    app.run()
