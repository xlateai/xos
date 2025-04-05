import xospy
import numpy as np
import time
import math
import random


RENDER_VIDEO = False
VELOCITY = 512

def get_webcam_frame() -> np.ndarray:
    cam_w, _ = xospy.video.webcam.get_resolution()
    cam_bytes = xospy.video.webcam.get_frame()
    bytes_per_pixel = 3
    total_pixels = len(cam_bytes) // bytes_per_pixel
    cam_h = total_pixels // cam_w

    if cam_w * cam_h * bytes_per_pixel != len(cam_bytes):
        raise Exception("Webcam resolution doesn't match buffer size. Skipping.")

    cam_array = np.frombuffer(cam_bytes, dtype=np.uint8).reshape((cam_h, cam_w, 3))
    return cam_array


def impose_frame(cam_frame: np.ndarray, frame: np.ndarray) -> np.ndarray:
    scale = min(frame.shape[1] / cam_frame.shape[1], frame.shape[0] / cam_frame.shape[0])
    new_w = int(cam_frame.shape[1] * scale)
    new_h = int(cam_frame.shape[0] * scale)
    print(f"Resized: {new_w}x{new_h} (scale {scale:.2f})")

    y_indices = (np.linspace(0, cam_frame.shape[0] - 1, new_h)).astype(int)
    x_indices = (np.linspace(0, cam_frame.shape[1] - 1, new_w)).astype(int)
    resized_cam = cam_frame[y_indices[:, None], x_indices]

    x_off = (frame.shape[1] - new_w) // 2
    y_off = (frame.shape[0] - new_h) // 2
    frame[y_off:y_off+new_h, x_off:x_off+new_w, :3] = resized_cam
    frame[y_off:y_off+new_h, x_off:x_off+new_w, 3] = 255  # alpha

    return frame


class Ball:
    def __init__(self, width, height):
        self.pos = np.array([width / 2, height / 2], dtype=float)
        self.angle = random.uniform(0, 2 * math.pi)
        self.radius = 30 * 0.85  # 15% smaller
        self.elapsed_time = 0.0

        self.target_angle = self._pick_new_angle()
        self.angle_lerp_speed = 0.5  # radians per second

    def _pick_new_angle(self):
        return random.uniform(0, 2 * math.pi)

    def update(self, dt, width, height):
        self.elapsed_time += dt

        # Velocity oscillates from 0.7x to 1.3x the base speed
        velocity_mod = 1.0 + 0.3 * math.sin(self.elapsed_time * 0.5)
        current_speed = VELOCITY * velocity_mod

        # Smoothly rotate toward target_angle
        angle_diff = (self.target_angle - self.angle + math.pi) % (2 * math.pi) - math.pi
        max_angle_step = self.angle_lerp_speed * dt
        if abs(angle_diff) < max_angle_step:
            self.angle = self.target_angle
            self.target_angle = self._pick_new_angle()
        else:
            self.angle += max(-max_angle_step, min(angle_diff, max_angle_step))

        # Move the ball
        delta = np.array([math.cos(self.angle), math.sin(self.angle)]) * current_speed * dt
        self.pos += delta

        # Bounce off walls
        if self.pos[0] - self.radius < 0:
            self.pos[0] = self.radius
            self.angle = math.pi - self.angle
        if self.pos[0] + self.radius > width:
            self.pos[0] = width - self.radius
            self.angle = math.pi - self.angle
        if self.pos[1] - self.radius < 0:
            self.pos[1] = self.radius
            self.angle = -self.angle
        if self.pos[1] + self.radius > height:
            self.pos[1] = height - self.radius
            self.angle = -self.angle

        self.angle %= 2 * math.pi

    def draw(self, frame):
        y, x = np.ogrid[:frame.shape[0], :frame.shape[1]]
        dist = np.sqrt((x - self.pos[0])**2 + (y - self.pos[1])**2)
        mask = dist <= self.radius
        frame[mask] = [0, 255, 0, 255]  # neon green


class PyApp(xospy.ApplicationBase):
    def setup(self, state):
        xospy.video.webcam.init_camera()
        self.last_time = time.time()
        self.tick_count = 0
        self.ball = Ball(state.frame.width, state.frame.height)

    def tick(self, state):
        self.tick_count += 1
        now = time.time()
        dt = now - self.last_time
        self.last_time = now

        width, height = state.frame.width, state.frame.height
        mv = memoryview(state.frame.buffer)
        frame = np.frombuffer(mv, dtype=np.uint8).reshape((height, width, 4))
        frame[:] = 0  # black background

        self.ball.update(dt, width, height)
        self.ball.draw(frame)

        if RENDER_VIDEO:
            cam_frame = get_webcam_frame()
            frame = impose_frame(cam_frame, frame)

        return frame


xospy.run_py_game(PyApp(), web=False, react_native=False)
