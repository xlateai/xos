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
        super().__init__()
        self.balls = []
    
    def setup(self):
        """Initialize the game"""
        # Create initial balls
        for _ in range(512):
            x = xos.random.uniform(BALL_RADIUS, self.get_width() - BALL_RADIUS)
            y = xos.random.uniform(BALL_RADIUS, self.get_height() - BALL_RADIUS)
            self.balls.append(BallState(x, y, BALL_RADIUS))
        
        xos.print("+512 balls (initial spawn)")
    
    def tick(self):
        """Update and render one frame"""
        # Update all balls
        for ball in self.balls:
            ball.update(self.get_width(), self.get_height())
        
        # Collect positions for fast rasterization
        positions = [(ball.x, ball.y) for ball in self.balls]
        radii = [ball.radius for ball in self.balls]
        
        # Use fast Rust rasterizer to draw all circles at once
        # Pass self.frame directly - the array data stays in Rust
        xos.rasterizer.circles(self.frame, positions, radii, BALL_COLOR)
    
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

