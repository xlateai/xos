import xos

# Configuration
NUM_LINES = 256
BASELINE_LENGTH = 0.012
MAX_EXTRA_LENGTH = 0.678
LINE_THICKNESS = 0.003
AMPLIFICATION_FACTOR = 50.0

# 🔥 THIS IS THE KEY KNOB
ADVANCE_PER_TICK = 8   # try 4–16 (higher = faster propagation)


class MicrophoneWaveform(xos.Application):
    def __init__(self):
        super().__init__()
        self.microphone = None

        # Buffers
        self.sample_buffer = [0.0] * NUM_LINES
        self.color_buffer = [(128, 128, 128, 255)] * NUM_LINES
        self.buffer_index = 0

    def setup(self):
        devices = xos.audio.get_input_devices()
        if not devices:
            raise RuntimeError("No audio input devices available")

        system_type = xos.system.get_system_type()

        if system_type == "IOS":
            device_id = 0
        else:
            device_names = [dev['name'] for dev in devices]
            device_id = xos.dialoguer.select(
                "Select microphone", device_names, default=0
            )

        # NOTE: buffer_duration can stay as-is
        self.microphone = xos.audio.Microphone(
            device_id=device_id,
            buffer_duration=0.05
        )

        xos.print("🔥 Fast Microphone Waveform initialized")

    def amplify_nonlinear(self, value):
        abs_val = abs(value)
        boosted = abs_val * AMPLIFICATION_FACTOR

        if boosted < 0.1:
            amplified = boosted * 2.0
        elif boosted < 1.0:
            amplified = 0.2 + (boosted - 0.1) * 1.5
        else:
            amplified = 0.2 + 1.35 + max(0.0, xos.math.log(boosted - 1.0)) * 0.4

        return -amplified if value < 0.0 else amplified

    def tick(self):
        if self.microphone is None:
            return

        width = self.get_width()
        height = self.get_height()

        xos.rasterizer.fill(self.frame, (8, 10, 15, 255))

        batch = self.microphone.get_batch(256)
        if not batch or not batch['_data']:
            return

        samples = batch['_data']
        rms = (sum(s * s for s in samples) / len(samples)) ** 0.5

        amplified = self.amplify_nonlinear(rms)
        normalized = max(0.0, min(1.0, amplified))
        color = self._compute_color(normalized)

        # 🚀 ADVANCE MULTIPLE STEPS PER TICK
        for _ in range(ADVANCE_PER_TICK):
            self.sample_buffer[self.buffer_index] = normalized
            self.color_buffer[self.buffer_index] = color
            self.buffer_index = (self.buffer_index + 1) % NUM_LINES

        self._render(width, height)

    def _compute_color(self, amp):
        if amp < 0.15:
            b = int(180 + amp / 0.15 * 75)
            return (b, b, b, 255)
        elif amp < 0.4:
            t = (amp - 0.15) / 0.25
            return (int(255 - t * 155), 255, 255, 255)
        elif amp < 0.65:
            t = (amp - 0.4) / 0.25
            return (int(100 - t * 100), 255, int(255 - t * 155), 255)
        elif amp < 0.85:
            t = (amp - 0.65) / 0.2
            return (int(t * 255), 255, 0, 255)
        else:
            t = (amp - 0.85) / 0.15
            return (255, int(255 - t * 100), 0, 255)

    def _render(self, width, height):
        center_x = width * 0.5
        spacing = height / NUM_LINES
        thickness_px = LINE_THICKNESS * height

        start_points = []
        end_points = []
        thicknesses = []
        colors = []

        for line_idx in range(NUM_LINES):
            buf_idx = (self.buffer_index + line_idx) % NUM_LINES
            amp = self.sample_buffer[buf_idx]

            half_len = (BASELINE_LENGTH + amp * MAX_EXTRA_LENGTH) * width * 0.5
            y = height - (line_idx * spacing)

            start_points.append((center_x - half_len, y))
            end_points.append((center_x + half_len, y))
            thicknesses.append(thickness_px)
            colors.append(self.color_buffer[buf_idx])

        xos.rasterizer.lines_batched(
            self.frame,
            start_points,
            end_points,
            thicknesses,
            colors
        )


if __name__ == "__main__":
    xos.print("🎤 Fast Microphone Waveform")
    app = MicrophoneWaveform()
    app.run()