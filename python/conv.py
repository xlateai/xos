import xos

class Convolution(xos.Application):
    def __init__(self):
        super().__init__()
        self.kernel = None
        self.needs_init = True
    
    def setup(self):
        """Initialize random 3x3x3 kernel"""
        xos.print("Convolution Demo initialized")
        
        # Generate random 3x3x3 kernel (RGB convolution kernel)
        # Each channel has its own 3x3 kernel
        kernel_size = 3
        self.kernel = xos.random.uniform(-1.0, 1.0, shape=(kernel_size, kernel_size, 3), dtype=xos.float32)
        xos.print(f"Generated random {kernel_size}x{kernel_size}x3 kernel")
        xos.print("Setup complete! Will generate initial image and start convolution...")
    
    def reset_state(self):
        """Reset state by generating new random image"""
        width = self.get_width()
        height = self.get_height()
        xos.print(f"Generating random {width}x{height} image...")
        xos.random.uniform_fill(self.frame.array, 0.0, 255.0)
    
    def tick(self):
        """Generate initial random image on first tick, then apply convolution"""
        # First tick: generate initial random state
        if self.needs_init:
            self.reset_state()
            xos.print("Starting convolution...")
            self.needs_init = False
            return
        
        # Every subsequent tick: apply convolution and update frame
        result = xos.ops.convolve(self.frame.array, self.kernel)
        self.frame.array[:] = result
    
    def on_screen_size_change(self, width, height):
        """Handle screen resize by resetting state"""
        xos.print(f"Screen resized to {width}x{height}")
        self.reset_state()


# Demo code
if __name__ == "__main__":
    xos.print("Convolution Demo")
    xos.print("Applies random 3x3x3 convolution kernel at each frame")
    
    app = Convolution()
    app.run()

