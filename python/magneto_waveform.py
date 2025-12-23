import xos

# Configuration
NUM_LINES = 256
BUFFER_SIZE = 256
NORM_WINDOW = 32

BASELINE_LENGTH = 0.04
MAX_EXTRA_LENGTH = 0.45
LINE_THICKNESS = 0.0012

EPSILON = 1e-6  # threshold for "non-zero" magnetometer


class MagnetoHorizontalWaveform(xos.Application):
    def __init__(self):
        super().__init__()
        self.magnetometer = None

        # Raw ring (for normalization window)
        self.mag_raw = [0.0] * BUFFER_SIZE
        self.x_raw = [0.0] * BUFFER_SIZE
        self.y_raw = [0.0] * BUFFER_SIZE
        self.z_raw = [0.0] * BUFFER_SIZE

        # Committed (write-once)
        self.mag_norm = [0.0] * BUFFER_SIZE
        self.colors = [(128, 128, 128, 255)] * BUFFER_SIZE

        self.buffer_index = 0
        self.sample_count = 0

        self.seeded = False
        self.waiting_for_signal = True  # 👈 KEY ADDITION

    def setup(self):
        self.magnetometer = xos.sensors.magnetometer()
        xos.print("Magneto Horizontal Waveform — waiting for first valid signal")

    def _compute_window_minmax(self, window_n):
        newest = (self.buffer_index - 1) % BUFFER_SIZE

        min_mag = max_mag = self.mag_raw[newest]
        min_x = max_x = self.x_raw[newest]
        min_y = max_y = self.y_raw[newest]
        min_z = max_z = self.z_raw[newest]

        idx = newest
        for _ in range(window_n - 1):
            idx = (idx - 1) % BUFFER_SIZE

            v = self.mag_raw[idx]
            if v < min_mag: min_mag = v
            if v > max_mag: max_mag = v

            v = self.x_raw[idx]
            if v < min_x: min_x = v
            if v > max_x: max_x = v

            v = self.y_raw[idx]
            if v < min_y: min_y = v
            if v > max_y: max_y = v

            v = self.z_raw[idx]
            if v < min_z: min_z = v
            if v > max_z: max_z = v

        return (min_mag, max_mag, min_x, max_x, min_y, max_y, min_z, max_z)

    def tick(self):
        mx, my, mz = self.magnetometer.read()
        mag_sq = mx * mx + my * my + mz * mz

        # --------------------------------------------------
        # WAIT until first non-zero magnetometer reading
        # --------------------------------------------------
        if self.waiting_for_signal:
            if mag_sq < EPSILON:
                return  # do nothing yet
            else:
                self.waiting_for_signal = False
                xos.print("Magnetometer active — starting waveform")

        magnitude = mag_sq ** 0.5

        # --------------------------------------------------
        # Seed buffers ONCE with first valid sample
        # --------------------------------------------------
        if not self.seeded:
            for k in range(BUFFER_SIZE):
                self.mag_raw[k] = magnitude
                self.x_raw[k] = mx
                self.y_raw[k] = my
                self.z_raw[k] = mz
                self.mag_norm[k] = 0.0
                self.colors[k] = (128, 128, 128, 255)

            self.seeded = True
            self.sample_count = 1
            self.buffer_index = 0

        # --------------------------------------------------
        # Write raw sample
        # --------------------------------------------------
        i = self.buffer_index
        self.mag_raw[i] = magnitude
        self.x_raw[i] = mx
        self.y_raw[i] = my
        self.z_raw[i] = mz

        self.buffer_index = (i + 1) % BUFFER_SIZE
        if self.sample_count < BUFFER_SIZE:
            self.sample_count += 1

        # --------------------------------------------------
        # Normalize using true rolling window
        # --------------------------------------------------
        window_n = min(self.sample_count, NORM_WINDOW)

        (min_mag, max_mag,
         min_x, max_x,
         min_y, max_y,
         min_z, max_z) = self._compute_window_minmax(window_n)

        def normalize(v, vmin, vmax, default):
            rng = vmax - vmin
            if rng < 1e-6:
                return default
            return (v - vmin) / rng

        wrote_idx = (self.buffer_index - 1) % BUFFER_SIZE

        self.mag_norm[wrote_idx] = normalize(magnitude, min_mag, max_mag, 0.0)

        r = int(normalize(mx, min_x, max_x, 0.5) * 255)
        g = int(normalize(my, min_y, max_y, 0.5) * 255)
        b = int(normalize(mz, min_z, max_z, 0.5) * 255)
        self.colors[wrote_idx] = (r, g, b, 255)

        # --------------------------------------------------
        # Render committed waveform
        # --------------------------------------------------
        width = self.get_width()
        height = self.get_height()

        center_x = width * 0.5
        spacing = height / NUM_LINES
        thickness_px = LINE_THICKNESS * height

        for line_idx in range(NUM_LINES):
            buf_idx = (self.buffer_index + line_idx) % BUFFER_SIZE

            half_len = (
                (BASELINE_LENGTH + self.mag_norm[buf_idx] * MAX_EXTRA_LENGTH)
                * width * 0.5
            )

            y = height - line_idx * spacing
            x0 = center_x - half_len
            x1 = center_x + half_len

            xos.rasterizer.lines(
                self.frame,
                [(x0, y)],
                [(x1, y)],
                [thickness_px],
                self.colors[buf_idx]
            )


if __name__ == "__main__":
    xos.print("Magnetometer Horizontal Waveform — gated start")
    game = MagnetoHorizontalWaveform()
    game.run()