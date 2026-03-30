"""
Depthwise convolution demo - one 3x3 kernel, applied to each RGB channel independently.

Frame is u8, kernel is float; normalized internally for correct u8 output.
"""
import xos


class DepthwiseConvolution(xos.Application):
    def __init__(self):
        super().__init__()
        self.kernel = None
        self.needs_init = True
    
    def setup(self):
        """Initialize random 3x3 depthwise kernel (once)"""
        xos.print("Depthwise Convolution Demo initialized")
        kernel_size = 3
        self.kernel = xos.random.uniform(-1.0, 1.0, shape=(kernel_size, kernel_size), dtype=xos.float32)
        xos.print(f"Generated random {kernel_size}x{kernel_size} depthwise kernel")
        xos.print("Setup complete!")
    
    def reset_state(self):
        """Generate fresh random image"""
        width = self.get_width()
        height = self.get_height()
        xos.print(f"Generating random {width}x{height} image...")
        xos.random.uniform_fill(self.frame.tensor, 0.0, 255.0)
    
    def tick(self):
        """First tick: init. Then: depthwise conv every frame."""
        if self.needs_init:
            self.reset_state()
            xos.print("Starting depthwise convolution...")
            self.needs_init = False
            return
        
        self.frame.tensor[:] = xos.ops.convolve_depthwise(self.frame.tensor, self.kernel)
    
    def on_screen_size_change(self, width, height):
        xos.print(f"Screen resized to {width}x{height}")
        self.reset_state()


if __name__ == "__main__":
    xos.print("Depthwise Convolution Demo")
    app = DepthwiseConvolution()
    app.run()
