import xospy
import numpy as np

class PyApp(xospy.ApplicationBase):
    def setup(self, state):
        self.counter = 0

    def tick(self, state):
        print(state.frame.height, state.frame.width, state.mouse.x, state.mouse.y, state.mouse.is_down)

        self.counter += 1
        width = state.frame.width
        height = state.frame.height

        print(self.counter)
        # Get the buffer as a memoryview (shared)
        mv = memoryview(state.frame.buffer)

        # Interpret as (height, width, 4) uint8 RGBA image
        frame = np.frombuffer(mv, dtype=np.uint8).reshape((height, width, 4))

        # Draw a red square in the center (200x200)
        square_size = 200
        x0 = width // 2 - square_size // 2
        y0 = height // 2 - square_size // 2
        x1 = x0 + square_size
        y1 = y0 + square_size

        frame[y0:y1, x0:x1, 0] = 255  # R
        frame[y0:y1, x0:x1, 1] = 0    # G
        frame[y0:y1, x0:x1, 2] = 0    # B
        frame[y0:y1, x0:x1, 3] = 255  # A

xospy.run_py_game(PyApp(), web=False, react_native=False)
