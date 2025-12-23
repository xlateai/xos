import xos

class RandomImage(xos.Application):
    def __init__(self):
        super().__init__()
        self.image_generated = False
    
    def setup(self):
        """Initialize the application"""
        xos.print("Random Image Generator initialized")
        xos.print("Generating static random image...")
    
    def tick(self):
        """Generate and display random image once"""
        
        # Get frame dimensions
        width = self.get_width()
        height = self.get_height()
        
        xos.print(f"Generating {width}x{height} random image...")
        
        # Generate random image data and update frame
        self.frame.array[:] = xos.random.uniform_fill(self.frame.array, 0.0, 255.0)
        
        xos.print("Random image displayed!")
        self.image_generated = True


# Demo code to show how it would be used
if __name__ == "__main__":
    xos.print("Random Image Display")
    xos.print("Displays a single randomly generated image")
    
    app = RandomImage()
    app.run()

