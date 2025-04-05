import xospy
import numpy as np

class PyApp(xospy.ApplicationBase):
    def setup(self, state):
        self.ball_y = 0.0
        self.vy = 0.0
        self.radius = 50
        self.gravity = 2.0

    def tick(self, state):
        width = state.frame.width
        height = state.frame.height

        # Update velocity and position
        self.vy += self.gravity
        self.ball_y += self.vy

        # Bounce off bottom
        if self.ball_y >= height - self.radius:
            self.ball_y = height - self.radius
            self.vy *= -1

        # Bounce off top
        if self.ball_y <= self.radius:
            self.ball_y = self.radius
            self.vy *= -1

        # Get and clear frame
        mv = memoryview(state.frame.buffer)
        frame = np.frombuffer(mv, dtype=np.uint8).reshape((height, width, 4))
        frame[:, :, :] = 0  # black background

        # Draw ball
        cx = width // 2
        cy = int(self.ball_y)
        r = self.radius

        y0 = max(cy - r, 0)
        y1 = min(cy + r, height)
        x0 = max(cx - r, 0)
        x1 = min(cx + r, width)

        for y in range(y0, y1):
            for x in range(x0, x1):
                if (x - cx) ** 2 + (y - cy) ** 2 <= r ** 2:
                    frame[y, x, 0] = 255  # R
                    frame[y, x, 1] = 0    # G
                    frame[y, x, 2] = 0    # B
                    frame[y, x, 3] = 255  # A

        return frame  # yes, returning the damn frame

xospy.run_py_game(PyApp(), web=False, react_native=False)
