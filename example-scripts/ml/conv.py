"""
Convolution demo - applies one random 3x3x3 kernel each frame.

Frame is u8, kernel is float; the conv op normalizes the kernel and maps
the float output correctly for u8 display.
"""
import xos


class Convolution(xos.Application):
    def __init__(self):
        super().__init__()
        self.kernel = None
        self.needs_init = True
    
    def setup(self):
        """Initialize random 3x3x3 kernel (once)"""
        xos.print("Convolution Demo initialized")
        kernel_size = 3
        self.kernel = xos.random.uniform(-1.0, 1.0, shape=(kernel_size, kernel_size, 3), dtype=xos.float32)
        xos.print(f"Generated random {kernel_size}x{kernel_size}x3 kernel")
        xos.print("Setup complete!")
    
    def reset_state(self):
        """Generate fresh random image"""
        width = self.get_width()
        height = self.get_height()
        xos.print(f"Generating random {width}x{height} image...")
        xos.random.uniform_fill(self.frame.tensor, 0.0, 255.0)
    
    def tick(self):
        """First tick: init image. Then: apply convolution every frame."""
        if self.needs_init:
            self.reset_state()
            xos.print("Starting convolution...")
            self.needs_init = False
            return
        
        result = xos.ops.convolve(self.frame.tensor, self.kernel)
        self.frame.tensor[:] = result
    
    def on_screen_size_change(self, width, height):
        xos.print(f"Screen resized to {width}x{height}")
        self.reset_state()


if __name__ == "__main__":
    xos.print("Convolution Demo")
    app = Convolution()
    app.run()
