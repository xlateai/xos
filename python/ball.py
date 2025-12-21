import xos

# Red ball color
BALL_COLOR = (255, 50, 50, 255)  # RGBA: Red
BALL_RADIUS = 15.0
SPEED_MULTIPLIER = 3.45

class BallState:
    def __init__(self, x, y, radius):
        self.x = x
        self.y = y
        self.radius = radius
        self.vx = xos.random.uniform(-2.0, 2.0) * SPEED_MULTIPLIER
        self.vy = xos.random.uniform(-2.0, 2.0) * SPEED_MULTIPLIER
    
    def update(self, width, height):
        self.x += self.vx
        self.y += self.vy
        
        # Check if ball is completely off screen
        is_off_screen = (
            self.x + self.radius < 0.0 or
            self.x - self.radius > width or
            self.y + self.radius < 0.0 or
            self.y - self.radius > height
        )
        
        if is_off_screen:
            # Respawn at center with the same heading
            self.x = width / 2.0
            self.y = height / 2.0
        else:
            # Normal bounce logic
            if self.x - self.radius < 0.0:
                self.x = self.radius
                self.vx = abs(self.vx)
            elif self.x + self.radius > width:
                self.x = width - self.radius
                self.vx = -abs(self.vx)
            
            if self.y - self.radius < 0.0:
                self.y = self.radius
                self.vy = abs(self.vy)
            elif self.y + self.radius > height:
                self.y = height - self.radius
                self.vy = -abs(self.vy)


class BallGame(xos.Application):
    def __init__(self):
        self.balls = []
        self.width = 0
        self.height = 0
    
    def setup(self):
        """Initialize the game"""
        # Get frame buffer info
        frame = xos.get_frame_buffer()
        self.width = frame["width"]
        self.height = frame["height"]
        
        # Create initial balls
        for _ in range(512):
            x = xos.random.uniform(BALL_RADIUS, self.width - BALL_RADIUS)
            y = xos.random.uniform(BALL_RADIUS, self.height - BALL_RADIUS)
            self.balls.append(BallState(x, y, BALL_RADIUS))
        
        xos.print("+512 balls (initial spawn)")
    
    def draw_circle(self, buffer, cx, cy, radius):
        """Draw a filled circle on the frame buffer - DEPRECATED, use rasterizer instead"""
        radius_squared = radius * radius
        
        start_x = max(0, int(cx - radius))
        end_x = min(self.width, int(cx + radius + 1))
        start_y = max(0, int(cy - radius))
        end_y = min(self.height, int(cy + radius + 1))
        
        for y in range(start_y, end_y):
            for x in range(start_x, end_x):
                dx = x - cx
                dy = y - cy
                if dx * dx + dy * dy <= radius_squared:
                    # Calculate buffer index (RGBA format)
                    idx = (y * self.width + x) * 4
                    if idx + 3 < len(buffer):
                        buffer[idx + 0] = BALL_COLOR[0]  # R
                        buffer[idx + 1] = BALL_COLOR[1]  # G
                        buffer[idx + 2] = BALL_COLOR[2]  # B
                        buffer[idx + 3] = BALL_COLOR[3]  # A
    
    def tick(self):
        """Update and render one frame"""
        # Get frame buffer
        frame = xos.get_frame_buffer()
        
        # Update all balls
        for ball in self.balls:
            ball.update(self.width, self.height)
        
        # Collect positions for fast rasterization
        positions = [(ball.x, ball.y) for ball in self.balls]
        radii = [ball.radius for ball in self.balls]
        
        # Use fast Rust rasterizer to draw all circles at once
        xos.rasterizer.circles(frame, positions, radii, BALL_COLOR)
    
    def on_mouse_down(self, x, y):
        """Handle mouse click"""
        self.balls.append(BallState(x, y, BALL_RADIUS))
        xos.print("+1 ball (click spawn)")


# Demo code to show how it would be used
if __name__ == "__main__":
    xos.print("Red Ball Game - Python Edition")
    xos.print("Click to spawn red balls!")
    
    game = BallGame()
    game.run()

