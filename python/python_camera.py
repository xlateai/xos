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
        frame[:] = 0  # clear to black

        # === Webcam frame ===
        cam_w, _ = xospy.video.webcam.get_resolution()  # only trust width
        cam_bytes = xospy.video.webcam.get_frame()
        bytes_per_pixel = 3
        total_pixels = len(cam_bytes) // bytes_per_pixel
        cam_h = total_pixels // cam_w
        print(f"Webcam: {cam_w}x{cam_h}, Bytes: {len(cam_bytes)}, Pixels: {total_pixels}")

        if cam_w * cam_h * bytes_per_pixel != len(cam_bytes):
            print("[WARNING] Webcam resolution doesn't match buffer size. Skipping.")
            return frame

        cam_array = np.frombuffer(cam_bytes, dtype=np.uint8).reshape((cam_h, cam_w, 3))

        # === Resize ===
        scale = min(width / cam_w, height / cam_h)
        new_w = int(cam_w * scale)
        new_h = int(cam_h * scale)
        print(f"Resized: {new_w}x{new_h} (scale {scale:.2f})")

        y_indices = (np.linspace(0, cam_h - 1, new_h)).astype(int)
        x_indices = (np.linspace(0, cam_w - 1, new_w)).astype(int)
        resized_cam = cam_array[y_indices[:, None], x_indices]

        # === Paste into frame ===
        x_off = (width - new_w) // 2
        y_off = (height - new_h) // 2
        frame[y_off:y_off+new_h, x_off:x_off+new_w, :3] = resized_cam
        frame[y_off:y_off+new_h, x_off:x_off+new_w, 3] = 255  # alpha

        return frame

xospy.run_py_game(PyApp(), web=False, react_native=False)
