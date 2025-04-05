import xospy
import numpy as np
import math
import time

class PyApp(xospy.ApplicationBase):
    def setup(self, state):
        self.num_particles = 256
        self.width = state.frame.width
        self.height = state.frame.height
        self.cx = self.width >> 1
        self.cy = self.height >> 1

        self.angles = np.random.uniform(0, 2 * np.pi, self.num_particles).astype(np.float32)
        self.radii = np.random.uniform(20, min(self.cx, self.cy) - 10, self.num_particles).astype(np.float32)
        self.speeds = np.random.uniform(0.001, 0.01, self.num_particles).astype(np.float32)

        rgb = np.random.randint(150, 255, (self.num_particles, 3), dtype=np.uint8)
        alpha = np.full((self.num_particles, 1), 255, dtype=np.uint8)
        self.colors = np.concatenate((rgb, alpha), axis=1)

        # Preallocate reusable arrays
        self.xs = np.empty(self.num_particles, dtype=np.int32)
        self.ys = np.empty(self.num_particles, dtype=np.int32)

        self.last_time = time.time()
        self.tick_count = 0

    def tick(self, state):
        self.tick_count += 1
        now = time.time()
        if now - self.last_time >= 1.0:
            print(f"TPS: {self.tick_count}")
            self.tick_count = 0
            self.last_time = now

        mv = memoryview(state.frame.buffer)
        frame = np.frombuffer(mv, dtype=np.uint8).reshape((self.height, self.width, 4))

        # In-place fade with integer math (faster than float -> int conversions)
        frame[:, :, 0] = (frame[:, :, 0] * 217) >> 8  # ~0.85 * 255 ≈ 217
        frame[:, :, 1] = (frame[:, :, 1] * 217) >> 8
        frame[:, :, 2] = (frame[:, :, 2] * 217) >> 8

        # Update angles and compute positions
        self.angles += self.speeds
        np.multiply(np.cos(self.angles), self.radii, out=self.xs, casting='unsafe')
        np.multiply(np.sin(self.angles), self.radii, out=self.ys, casting='unsafe')
        self.xs += self.cx
        self.ys += self.cy

        # Convert to int32 in-place
        self.xs = self.xs.astype(np.int32, copy=False)
        self.ys = self.ys.astype(np.int32, copy=False)

        # Filter in-bounds
        mask = (self.xs >= 1) & (self.xs < self.width - 1) & (self.ys >= 1) & (self.ys < self.height - 1)
        xs = self.xs[mask]
        ys = self.ys[mask]
        colors = self.colors[mask]

        # Draw 3x3 blobs using broadcasting
        for dx in (-1, 0, 1):
            for dy in (-1, 0, 1):
                frame[ys + dy, xs + dx] = colors

        return frame

xospy.run_py_game(PyApp(), web=False, react_native=False)
