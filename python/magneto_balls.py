import xos

# Configuration
BALL_RADIUS = 0.005
SPEED_MULTIPLIER = 0.005
NUM_BALLS = 512

class MagnetoBalls(xos.Application):
    def __init__(self):
        super().__init__()
        self.num_balls = NUM_BALLS
        self.positions = None
        self.velocities = None
        self.radii = None
        self.magnetometer = None
        
        # Simple running average
        self.mag_x = 0.0
        self.mag_y = 0.0
        self.mag_z = 0.0
    
    def setup(self):
        """Initialize the game and magnetometer"""
        # Initialize magnetometer
        self.magnetometer = xos.sensors.magnetometer()
        
        # Initialize balls (same as ball.py)
        initial_positions = []
        initial_radii = []
        initial_velocities = []
        
        for _ in range(self.num_balls):
            x = xos.random.uniform(BALL_RADIUS, 1.0 - BALL_RADIUS)
            y = xos.random.uniform(BALL_RADIUS, 1.0 - BALL_RADIUS)
            initial_positions.append([x, y])
            initial_radii.append(BALL_RADIUS)
            
            vx = xos.random.uniform(-2.0, 2.0) * SPEED_MULTIPLIER
            vy = xos.random.uniform(-2.0, 2.0) * SPEED_MULTIPLIER
            initial_velocities.append([vx, vy])
        
        self.positions = xos.array(initial_positions, (self.num_balls, 2))
        self.radii = xos.array(initial_radii, (self.num_balls,))
        self.velocities = xos.array(initial_velocities, (self.num_balls, 2))
        
        xos.print(f"+{self.num_balls} magneto balls spawned!")
    
    def tick(self):
        """Update and render one frame"""
        # Read magnetometer and smooth it
        x, y, z = self.magnetometer.read()
        
        # Simple exponential moving average
        alpha = 0.1
        self.mag_x = alpha * x + (1 - alpha) * self.mag_x
        self.mag_y = alpha * y + (1 - alpha) * self.mag_y
        self.mag_z = alpha * z + (1 - alpha) * self.mag_z
        
        # Calculate magnitude
        magnitude = (self.mag_x**2 + self.mag_y**2 + self.mag_z**2) ** 0.5
        
        # Map to color (Earth's field is typically 25-65 µT)
        # Scale to 0-255 range
        red = int(min(255, abs(self.mag_x) * 3))
        green = int(min(255, magnitude * 2))
        blue = int(min(255, abs(self.mag_z) * 3))
        ball_color = (red, green, blue, 255)
        
        # Update ball positions (same as ball.py)
        pos_data = self.positions["_data"]
        vel_data = self.velocities["_data"]
        
        for i in range(self.num_balls):
            # Update position
            pos_data[i*2] += vel_data[i*2]
            pos_data[i*2+1] += vel_data[i*2+1]
            
            # Bounce off edges
            radius = self.radii["_data"][i]
            if pos_data[i*2] - radius < 0.0 or pos_data[i*2] + radius > 1.0:
                vel_data[i*2] *= -1
                pos_data[i*2] = max(radius, min(1.0 - radius, pos_data[i*2]))
            if pos_data[i*2+1] - radius < 0.0 or pos_data[i*2+1] + radius > 1.0:
                vel_data[i*2+1] *= -1
                pos_data[i*2+1] = max(radius, min(1.0 - radius, pos_data[i*2+1]))
        
        # Convert to pixel coordinates
        width = self.get_width()
        height = self.get_height()
        
        pixel_positions = []
        for i in range(self.num_balls):
            px = pos_data[i*2] * width
            py = pos_data[i*2+1] * height
            pixel_positions.append((px, py))
        
        max_dimension = max(width, height)
        radii_list = [BALL_RADIUS * max_dimension for _ in range(self.num_balls)]
        
        # Render balls with magnetometer-based color
        xos.rasterizer.circles(self.frame, pixel_positions, radii_list, ball_color)


if __name__ == "__main__":
    xos.print("Magneto Balls - Move your phone to change colors!")
    game = MagnetoBalls()
    game.run()
