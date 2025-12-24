import xos

class DepthwiseConvolution(xos.Application):
    def __init__(self):
        super().__init__()
        self.kernel = None
        self.needs_init = True
    
    def setup(self):
        """Initialize random 3x3 depthwise kernel"""
        xos.print("Depthwise Convolution Demo initialized")
        
        # Generate random 3x3 kernel (same kernel applied to each RGB channel)
        kernel_size = 3
        self.kernel = xos.random.uniform(-0.2, 0.2, shape=(kernel_size, kernel_size), dtype=xos.float32)
        xos.print(f"Generated random {kernel_size}x{kernel_size} depthwise kernel")
        xos.print("Setup complete! Will generate initial image and start convolution...")
    
    def tick(self):
        """Generate initial random image on first tick, then apply depthwise convolution"""
        # First tick: generate initial random state
        if self.needs_init:
            width = self.get_width()
            height = self.get_height()
            xos.print(f"Generating initial random {width}x{height} image...")
            xos.random.uniform_fill(self.frame.array, 0.0, 255.0)
            xos.print("Starting depthwise convolution...")
            self.needs_init = False
            return
        
        # Every subsequent tick: apply depthwise convolution and update frame
        self.frame.array[:] = xos.ops.convolve_depthwise(self.frame.array, self.kernel)
    
    def on_screen_size_change(self, width, height):
        """Handle screen resize by regenerating random image"""
        xos.print(f"Screen resized to {width}x{height}, regenerating image...")
        xos.random.uniform_fill(self.frame.array, 0.0, 255.0)


# Demo code
if __name__ == "__main__":
    xos.print("Depthwise Convolution Demo")
    xos.print("Applies random 3x3 depthwise convolution kernel at each frame")
    xos.print("(Each color channel processed independently)")
    
    app = DepthwiseConvolution()
    app.run()

