import xos

# =========================
# Configuration
# =========================
NUM_LINES = 256
BASELINE_LENGTH = 0.012
MAX_EXTRA_LENGTH = 0.678
LINE_THICKNESS = 0.003

ADVANCE_PER_TICK = 8
AMPLIFICATION_FACTOR = 10.0

# MagnetoBalls-style color buffer
COLOR_BUFFER_SIZE = 128
PERCENTILE = 0.95

# Envelope tuning
BASELINE_ALPHA = 0.002
ATTACK = 0.35
DECAY = 0.08

# Phase texture
PHASE_STEP = 0.15
PHASE_DEPTH = 0.07

# 🔑 Adaptive normalization tuning
ENV_MAX_RISE = 0.02     # how fast max grows
ENV_MAX_FALL = 0.001    # how fast max shrinks
ENV_MIN_RISE = 0.001
ENV_MIN_FALL = 0.01


class MagnetometerWaveform(xos.Application):
    def __init__(self):
        super().__init__()
        self.magnetometer = None

        # Waveform buffers
        self.sample_buffer = [0.0] * NUM_LINES
        self.color_buffer = [(128, 128, 128, 255)] * NUM_LINES
        self.buffer_index = 0

        # MagnetoBalls RGB buffers
        self.mag_x = [0.0] * COLOR_BUFFER_SIZE
        self.mag_y = [0.0] * COLOR_BUFFER_SIZE
        self.mag_z = [0.0] * COLOR_BUFFER_SIZE
        self.color_index = 0

        # Envelope + phase
        self.baseline = None
        self.envelope = 0.0
        self.phase = 0.0

        # Adaptive normalization state
        self.env_min = None
        self.env_max = None
        self.magnetometer = xos.sensors.magnetometer()
        xos.print("🧲⚡ Magnetometer Waveform — adaptive normalized")

    # --------------------------------------------------
    # Percentile helper
    # --------------------------------------------------
    def percentile(self, data, p):
        s = sorted(data)
        idx = int(len(s) * p)
        return s[min(idx, len(s) - 1)]

    # --------------------------------------------------
    # MagnetoBalls RGB (95%)
    # --------------------------------------------------
    def compute_color(self, mx, my, mz):
        i = self.color_index
        self.mag_x[i] = mx
        self.mag_y[i] = my
        self.mag_z[i] = mz
        self.color_index = (i + 1) % COLOR_BUFFER_SIZE

        min_x, max_x = min(self.mag_x), self.percentile(self.mag_x, PERCENTILE)
        min_y, max_y = min(self.mag_y), self.percentile(self.mag_y, PERCENTILE)
        min_z, max_z = min(self.mag_z), self.percentile(self.mag_z, PERCENTILE)

        def norm(v, vmin, vmax):
            if vmax - vmin < 1e-3:
                return 0.5
            return max(0.0, min(1.0, (v - vmin) / (vmax - vmin)))

        return (
            int(norm(mx, min_x, max_x) * 255),
            int(norm(my, min_y, max_y) * 255),
            int(norm(mz, min_z, max_z) * 255),
            255
        )

    # --------------------------------------------------
    # Mic-style nonlinear amplification
    # --------------------------------------------------
    def amplify(self, value):
        v = value * AMPLIFICATION_FACTOR
        if v < 0.05:
            return v * 2.0
        elif v < 0.3:
            return 0.1 + (v - 0.05) * 1.6
        else:
            return 0.4 + xos.math.log(v + 1.0) * 0.35

    # --------------------------------------------------
    # Main tick
    # --------------------------------------------------
    def tick(self):
        width = self.get_width()
        height = self.get_height()

        xos.rasterizer.fill(self.frame, (8, 10, 15, 255))

        mx, my, mz = self.magnetometer.read()
        mag = xos.math.sqrt(mx * mx + my * my + mz * mz)

        # Initialize baseline
        if self.baseline is None:
            self.baseline = mag
            return

        # DC removal
        self.baseline += (mag - self.baseline) * BASELINE_ALPHA
        delta = abs(mag - self.baseline)

        # Envelope target
        target = self.amplify(delta)
        target = max(0.0, target)

        # Envelope follower
        if target > self.envelope:
            self.envelope += (target - self.envelope) * ATTACK
        else:
            self.envelope += (target - self.envelope) * DECAY

        # ----------------------------------------------
        # 🔑 Adaptive normalization
        # ----------------------------------------------
        if self.env_min is None:
            self.env_min = self.envelope
            self.env_max = self.envelope

        # Track max
        if self.envelope > self.env_max:
            self.env_max += (self.envelope - self.env_max) * ENV_MAX_RISE
        else:
            self.env_max += (self.envelope - self.env_max) * ENV_MAX_FALL

        # Track min
        if self.envelope < self.env_min:
            self.env_min += (self.envelope - self.env_min) * ENV_MIN_FALL
        else:
            self.env_min += (self.envelope - self.env_min) * ENV_MIN_RISE

        # Normalize envelope
        rng = self.env_max - self.env_min
        if rng < 1e-6:
            norm_env = 0.0
        else:
            norm_env = (self.envelope - self.env_min) / rng
            norm_env = max(0.0, min(1.0, norm_env))

        color = self.compute_color(mx, my, mz)

        # ----------------------------------------------
        # Fast advance with phase texture
        # ----------------------------------------------
        phase = self.phase
        for _ in range(ADVANCE_PER_TICK):
            mod = 1.0 + PHASE_DEPTH * xos.math.sin(phase)
            amp = norm_env * mod
            amp = max(0.0, min(1.0, amp))

            self.sample_buffer[self.buffer_index] = amp
            self.color_buffer[self.buffer_index] = color
            self.buffer_index = (self.buffer_index + 1) % NUM_LINES

            phase += PHASE_STEP

        self.phase = phase

        self.render(width, height)

    # --------------------------------------------------
    # Renderer
    # --------------------------------------------------
    def render(self, width, height):
        center_x = width * 0.5
        spacing = height / NUM_LINES
        thickness_px = LINE_THICKNESS * height

        starts, ends, thicknesses, colors = [], [], [], []

        for i in range(NUM_LINES):
            idx = (self.buffer_index + i) % NUM_LINES
            amp = self.sample_buffer[idx]

            half_len = (BASELINE_LENGTH + amp * MAX_EXTRA_LENGTH) * width * 0.5
            y = height - i * spacing

            starts.append((center_x - half_len, y))
            ends.append((center_x + half_len, y))
            thicknesses.append(thickness_px)
            colors.append(self.color_buffer[idx])

        xos.rasterizer.lines_batched(
            self.frame,
            starts,
            ends,
            thicknesses,
            colors
        )


if __name__ == "__main__":
    xos.print("🧲⚡ Magnetometer Waveform — self-normalizing")
    app = MagnetometerWaveform()
    app.run()