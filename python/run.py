import xospy
import numpy as np
import math
import random

class PyApp(xospy.ApplicationBase):
    def setup(self, state):
        self.num_particles = 256
        self.particles = []

        for _ in range(self.num_particles):
            angle = random.uniform(0, 2 * math.pi)
            radius = random.uniform(20, min(state.frame.width, state.frame.height) // 2 - 10)
            speed = random.uniform(0.001, 0.01)
            color = [random.randint(150, 255) for _ in range(3)] + [255]
            self.particles.append({
                'angle': angle,
                'radius': radius,
                'speed': speed,
                'color': color
            })

        self.center_x = state.frame.width // 2
        self.center_y = state.frame.height // 2

    def tick(self, state):
        width = state.frame.width
        height = state.frame.height
        mv = memoryview(state.frame.buffer)
        frame = np.frombuffer(mv, dtype=np.uint8).reshape((height, width, 4))

        # Fade old pixels
        frame[:, :, :3] = (frame[:, :, :3] * 0.85).astype(np.uint8)

        for p in self.particles:
            p['angle'] += p['speed']
            x = int(self.center_x + math.cos(p['angle']) * p['radius'])
            y = int(self.center_y + math.sin(p['angle']) * p['radius'])

            # Draw a small 3x3 pixel "blob" for visibility
            for dy in range(-1, 2):
                for dx in range(-1, 2):
                    px = x + dx
                    py = y + dy
                    if 0 <= px < width and 0 <= py < height:
                        frame[py, px] = p['color']

        return frame

xospy.run_py_game(PyApp(), web=False, react_native=False)
