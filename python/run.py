import xospy
import numpy as np
import time

class PyApp(xospy.ApplicationBase):
    def setup(self, state):
        self.num_particles = 256

        self.angles = np.random.uniform(0, 2 * np.pi, self.num_particles).astype(np.float32)
        self.radii = np.random.uniform(20, min(state.frame.width, state.frame.height) // 2 - 10, self.num_particles).astype(np.float32)
        self.speeds = np.random.uniform(0.001, 0.01, self.num_particles).astype(np.float32)

        rgb = np.random.randint(150, 255, (self.num_particles, 3), dtype=np.uint8)
        alpha = np.full((self.num_particles, 1), 255, dtype=np.uint8)
        self.colors = np.concatenate((rgb, alpha), axis=1)

        # Will be allocated on first tick
        self.frame_shape = None
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

        width = state.frame.width
        height = state.frame.height
        cx, cy = width >> 1, height >> 1

        mv = memoryview(state.frame.buffer)
        frame = np.frombuffer(mv, dtype=np.uint8).reshape((height, width, 4))

        # Realloc working state only if window size changed
        if self.frame_shape != frame.shape:
            self.frame_shape = frame.shape
            self.frame = frame  # reuse this view
            self.width = width
            self.height = height
            self.cx = cx
            self.cy = cy

        # Integer fade (0.85 * 255 ≈ 217)
        frame[:, :, 0] = (frame[:, :, 0] * 217) >> 8
        frame[:, :, 1] = (frame[:, :, 1] * 217) >> 8
        frame[:, :, 2] = (frame[:, :, 2] * 217) >> 8

        # Update positions
        self.angles += self.speeds
        np.multiply(np.cos(self.angles), self.radii, out=self.xs, casting='unsafe')
        np.multiply(np.sin(self.angles), self.radii, out=self.ys, casting='unsafe')
        self.xs += self.cx
        self.ys += self.cy

        self.xs = self.xs.astype(np.int32, copy=False)
        self.ys = self.ys.astype(np.int32, copy=False)

        # Mask in-bounds
        mask = (self.xs >= 1) & (self.xs < width - 1) & (self.ys >= 1) & (self.ys < height - 1)
        xs = self.xs[mask]
        ys = self.ys[mask]
        colors = self.colors[mask]

        # Fast 3x3 blob draw
        for dx in (-1, 0, 1):
            for dy in (-1, 0, 1):
                frame[ys + dy, xs + dx] = colors

        return frame

xospy.run_py_game(PyApp(), web=False, react_native=False)
