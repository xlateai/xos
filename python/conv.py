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
        kernel_flat = []
        for _ in range(3):  # height
            for _ in range(3):  # width
                for _ in range(3):  # channels (RGB)
                    kernel_flat.append(xos.random.uniform(-0.2, 0.2))
        
        # Normalize kernel so it doesn't darken/brighten the image too much
        # Add identity in the center to preserve original image somewhat
        center_idx = (1 * 3 + 1) * 3  # Middle of 3x3, start of RGB triplet
        kernel_flat[center_idx + 0] += 0.8  # Red center
        kernel_flat[center_idx + 1] += 0.8  # Green center
        kernel_flat[center_idx + 2] += 0.8  # Blue center
        
        self.kernel = kernel_flat
        xos.print(f"Generated random 3x3x3 kernel (normalized)")
        xos.print("Setup complete! Will generate initial image and start convolution...")
    
    def tick(self):
        """Generate initial random image on first tick, then apply convolution"""
        # First tick: generate initial random state
        if self.needs_init:
            width = self.get_width()
            height = self.get_height()
            xos.print(f"Generating initial random {width}x{height} image...")
            xos.random.uniform_fill(self.frame.array, 0.0, 255.0)
            xos.print("Starting convolution...")
            self.needs_init = False
            return
        
        # Every subsequent tick: apply convolution and update frame
        result = xos.ops.convolve(self.frame.array, self.kernel)
        # xos.print(f"Convolution result: {result}")
        self.frame.array[:] = result
        # print(result)


# Demo code
if __name__ == "__main__":
    xos.print("Convolution Demo")
    xos.print("Applies random 3x3x3 convolution kernel at each frame")
    
    app = Convolution()
    app.run()

