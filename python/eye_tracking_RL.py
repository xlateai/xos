import xospy
import numpy as np
import time

class PyApp(xospy.ApplicationBase):
    def setup(self, state):
        xospy.video.webcam.init_camera()
        self.last_time = time.time()
        self.tick_count = 0

    def tick(self, state):
        self.tick_count += 1
        now = time.time()
        if now - self.last_time >= 1.0:
            print(f"TPS: {self.tick_count}")
            self.tick_count = 0
            self.last_time = now

        width, height = state.frame.width, state.frame.height
        mv = memoryview(state.frame.buffer)
        frame = np.frombuffer(mv, dtype=np.uint8).reshape((height, width, 4))
        frame[:] = 0  # fill with black initially

        # Get webcam frame as RGB and shape
        cam_bytes = xospy.video.webcam.get_frame()
        cam_array = np.frombuffer(cam_bytes, dtype=np.uint8)
        cam_array = cam_array.reshape((-1, 3))  # RGB

        # Infer webcam resolution (you can also get this from get_resolution())
        cam_h = int((len(cam_array) / 3) ** 0.5)
        cam_w = len(cam_array) // cam_h
        cam_array = cam_array[:cam_h * cam_w].reshape((cam_h, cam_w, 3))

        # Resize webcam frame to fit inside main frame, maintaining aspect ratio
        scale = min(width / cam_w, height / cam_h)
        new_w = int(cam_w * scale)
        new_h = int(cam_h * scale)

        # Downscale or upscale with nearest neighbor manually (no PIL)
        y_indices = (np.linspace(0, cam_h - 1, new_h)).astype(int)
        x_indices = (np.linspace(0, cam_w - 1, new_w)).astype(int)
        resized_cam = cam_array[y_indices[:, None], x_indices]  # shape (new_h, new_w, 3)

        # Compute offsets to center
        y_off = (height - new_h) // 2
        x_off = (width - new_w) // 2

        # Paste resized camera image into the center of the frame
        frame[y_off:y_off+new_h, x_off:x_off+new_w, :3] = resized_cam
        frame[y_off:y_off+new_h, x_off:x_off+new_w, 3] = 255  # set alpha

        return frame

xospy.run_py_game(PyApp(), web=False, react_native=False)
