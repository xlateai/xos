# ball.py - example code for xos (`xos python code.py` in terminal to run)
import xos

BALL_COLOR = (255, 50, 50, 255)
BALL_RADIUS = 0.03

class BallDemo(xos.Application):
    headless: bool = False  # optional flag to disable viewport display (helpful for ml/rl)

    def setup(self):
        self.x, self.y = 0.5, 0.5
        self.vx, self.vy = 0.006, 0.004

    def tick(self):
        self.frame.clear(xos.color.BLACK)
        self.x += self.vx
        self.y += self.vy
        if self.x - BALL_RADIUS < 0 or self.x + BALL_RADIUS > 1:
            self.vx *= -1
            self.x = max(BALL_RADIUS, min(1 - BALL_RADIUS, self.x))
        if self.y - BALL_RADIUS < 0 or self.y + BALL_RADIUS > 1:
            self.vy *= -1
            self.y = max(BALL_RADIUS, min(1 - BALL_RADIUS, self.y))
        w, h = self.get_width(), self.get_height()
        r = BALL_RADIUS * max(w, h)
        xos.rasterizer.circles(self.frame, [(self.x * w, self.y * h)], [r], BALL_COLOR)

if __name__ == "__main__":
    BallDemo().run()

