import xospy
import numpy as np
import math
import random
import time

class PyApp(xospy.ApplicationBase):
    def setup(self, state):
        self.num_particles = 256

        self.angles = np.random.uniform(0, 2 * np.pi, self.num_particles).astype(np.float32)
        self.radii = np.random.uniform(20, min(state.frame.width, state.frame.height) // 2 - 10, self.num_particles).astype(np.float32)
        self.speeds = np.random.uniform(0.001, 0.01, self.num_particles).astype(np.float32)
        self.colors = np.random.randint(150, 255, (self.num_particles, 3), dtype=np.uint8)
        self.colors = np.concatenate([self.colors, np.full((self.num_particles, 1), 255, dtype=np.uint8)], axis=1)

        self.last_time = time.time()
        self.tick_count = 0

    def tick(self, state):
        self.tick_count += 1
        now = time.time()
        if now - self.last_time >= 1.0:
            print(f"TPS: {self.tick_count}")
            self.tick_count = 0
            self.last_time = now

        width = state.frame.width
        height = state.frame.height
        cx, cy = width // 2, height // 2

        mv = memoryview(state.frame.buffer)
        frame = np.frombuffer(mv, dtype=np.uint8).reshape((height, width, 4))

        # Fade old pixels
        frame[:, :, :3] = (frame[:, :, :3] * 0.85).astype(np.uint8)

        # Update angles
        self.angles += self.speeds

        # Compute new positions
        xs = (cx + np.cos(self.angles) * self.radii).astype(np.int32)
        ys = (cy + np.sin(self.angles) * self.radii).astype(np.int32)

        # Mask for in-bounds particles
        in_bounds = (xs >= 1) & (xs < width - 1) & (ys >= 1) & (ys < height - 1)
        xs = xs[in_bounds]
        ys = ys[in_bounds]
        colors = self.colors[in_bounds]

        # Vectorized 3x3 blob drawing
        for dx in [-1, 0, 1]:
            for dy in [-1, 0, 1]:
                x_draw = xs + dx
                y_draw = ys + dy
                frame[y_draw, x_draw] = colors

        return frame

xospy.run_py_game(PyApp(), web=False, react_native=False)
