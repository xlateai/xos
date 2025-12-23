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
        kernel_flat = []
        for _ in range(3):  # height
            for _ in range(3):  # width
                kernel_flat.append(xos.random.uniform(-0.2, 0.2))
        
        # Normalize kernel - add identity in center
        center_idx = 1 * 3 + 1  # Middle of 3x3
        kernel_flat[center_idx] += 0.8
        
        self.kernel = kernel_flat
        xos.print(f"Generated random 3x3 depthwise kernel (normalized)")
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


# Demo code
if __name__ == "__main__":
    xos.print("Depthwise Convolution Demo")
    xos.print("Applies random 3x3 depthwise convolution kernel at each frame")
    xos.print("(Each color channel processed independently)")
    
    app = DepthwiseConvolution()
    app.run()

