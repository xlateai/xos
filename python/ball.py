import xos

# Red ball color (normalized 0-1)
BALL_COLOR = (255, 50, 50, 255)  # RGBA: Red
BALL_RADIUS = 0.005
SPEED_MULTIPLIER = 0.005  # Normalized speed


class BallGame(xos.Application):
    def __init__(self):
        super().__init__()
        self.positions = None  # Will be Rust-backed array
        self.radii = None  # Will be Rust-backed array
        self.num_balls = 512
    
    def setup(self):
        """Initialize the game"""
        # Pre-allocate Rust-backed arrays for positions (Nx2) and radii (N)
        initial_positions = []
        initial_radii = []
        
        for _ in range(self.num_balls):
            x = xos.random.uniform(BALL_RADIUS, 1.0 - BALL_RADIUS)
            y = xos.random.uniform(BALL_RADIUS, 1.0 - BALL_RADIUS)
            initial_positions.append([x, y])
            initial_radii.append(BALL_RADIUS)
        
        # Create Rust-backed arrays
        self.positions = xos.array(initial_positions, (self.num_balls, 2))
        self.radii = xos.array(initial_radii, (self.num_balls,))
        
        # Velocities (vx, vy) for each ball
        initial_velocities = []
        for _ in range(self.num_balls):
            vx = xos.random.uniform(-2.0, 2.0) * SPEED_MULTIPLIER
            vy = xos.random.uniform(-2.0, 2.0) * SPEED_MULTIPLIER
            initial_velocities.append([vx, vy])
        self.velocities = xos.array(initial_velocities, (self.num_balls, 2))
        
        xos.print("+512 balls (initial spawn)")
        
        # Print first 8 positions to verify array slicing
        # print("Initial positions[:8]:")
        # print(self.positions["_data"][:16])  # First 8 balls = 16 floats (x,y pairs)
    
    def tick(self):
        """Update and render one frame"""
        # Clear the frame (no longer auto-cleared)
        xos.rasterizer.clear()
        
        # TODO: Vectorized update in Rust
        # For now, update positions manually
        pos_data = self.positions["_data"]
        vel_data = self.velocities["_data"]
        
        for i in range(self.num_balls):
            # Update position
            pos_data[i*2] += vel_data[i*2]       # x += vx
            pos_data[i*2+1] += vel_data[i*2+1]   # y += vy
            
            # Bounce off edges (keep ball fully on screen)
            radius = self.radii["_data"][i]
            if pos_data[i*2] - radius < 0.0 or pos_data[i*2] + radius > 1.0:
                vel_data[i*2] *= -1
                pos_data[i*2] = max(radius, min(1.0 - radius, pos_data[i*2]))
            if pos_data[i*2+1] - radius < 0.0 or pos_data[i*2+1] + radius > 1.0:
                vel_data[i*2+1] *= -1
                pos_data[i*2+1] = max(radius, min(1.0 - radius, pos_data[i*2+1]))
        
        # Print first 8 positions each tick
        # print("positions[:8] =", pos_data[:16])
        
        # Convert normalized positions to pixel coordinates for rasterizer
        width = self.get_width()
        height = self.get_height()
        
        pixel_positions = []
        for i in range(self.num_balls):
            px = pos_data[i*2] * width
            py = pos_data[i*2+1] * height
            pixel_positions.append((px, py))
        
        # Use the wider dimension for consistent ball size across orientations
        max_dimension = max(width, height)
        radii_list = [BALL_RADIUS * max_dimension for _ in range(self.num_balls)]  # Convert normalized radius to pixels
        
        # Use fast Rust rasterizer
        xos.rasterizer.circles(self.frame, pixel_positions, radii_list, BALL_COLOR)
    
    def on_mouse_down(self, x, y):
        """Handle mouse click"""
        # TODO: Dynamically grow arrays
        xos.print("+1 ball (click spawn) - dynamic growth not yet implemented")


# Demo code to show how it would be used
if __name__ == "__main__":
    xos.print("Red Ball Game - Python Edition")
    xos.print("Click to spawn red balls!")
    
    game = BallGame()
    game.run()

