import xos

# Configuration
NUM_LINES = 512
BUFFER_SIZE = 512
NORM_WINDOW = 32

BASELINE_LENGTH = 0.012
MAX_EXTRA_LENGTH = 0.678
LINE_THICKNESS = 0.003

EPSILON = 1e-6  # threshold for "non-zero" audio signal


class MicrophoneWaveform(xos.Application):
    def __init__(self):
        super().__init__()
        self.microphone = None

        # Raw ring (for normalization window)
        self.amplitude_raw = [0.0] * BUFFER_SIZE

        # Committed (write-once)
        self.amplitude_norm = [0.0] * BUFFER_SIZE
        self.colors = [(128, 128, 128, 255)] * BUFFER_SIZE

        self.buffer_index = 0
        self.sample_count = 0

        self.seeded = False
        self.waiting_for_signal = True

    def setup(self):
        # Print available input devices
        devices = xos.audio.get_input_devices()
        if devices is None:
            xos.print("ERROR: get_input_devices() returned None")
            raise RuntimeError("Failed to get audio devices")
        
        xos.print(f"Available audio input devices ({len(devices)} found):")
        for dev in devices:
            xos.print(f"  [{dev['id']}] {dev['name']}")
        
        if not devices:
            xos.print("WARNING: No audio input devices found!")
            raise RuntimeError("No audio input devices available")
        
        # Initialize microphone with first device
        try:
            self.microphone = xos.audio.Microphone(device_id=0, buffer_duration=1.0)
            xos.print("Microphone Waveform — waiting for first valid signal")
        except Exception as e:
            xos.print(f"Failed to initialize microphone: {e}")
            raise

    def _compute_window_minmax(self, window_n):
        newest = (self.buffer_index - 1) % BUFFER_SIZE

        min_amp = max_amp = self.amplitude_raw[newest]

        idx = newest
        for _ in range(window_n - 1):
            idx = (idx - 1) % BUFFER_SIZE

            v = self.amplitude_raw[idx]
            if v < min_amp: min_amp = v
            if v > max_amp: max_amp = v

        return (min_amp, max_amp)

    def tick(self):
        # Get a batch of samples from the microphone
        batch = xos.audio._microphone_get_batch(self.microphone._listener_ptr, 256)
        
        if not batch:
            return
        
        # Calculate RMS (root mean square) amplitude from the batch
        sum_squares = sum(s * s for s in batch)
        rms = (sum_squares / len(batch)) ** 0.5 if batch else 0.0
        
        # --------------------------------------------------
        # WAIT until first non-zero audio signal
        # --------------------------------------------------
        if self.waiting_for_signal:
            if rms < EPSILON:
                return  # do nothing yet
            else:
                self.waiting_for_signal = False
                xos.print("Microphone active — starting waveform")

        # --------------------------------------------------
        # Seed buffers ONCE with first valid sample
        # --------------------------------------------------
        if not self.seeded:
            for k in range(BUFFER_SIZE):
                self.amplitude_raw[k] = rms
                self.amplitude_norm[k] = 0.0
                self.colors[k] = (128, 128, 128, 255)

            self.seeded = True
            self.sample_count = 1
            self.buffer_index = 0

        # --------------------------------------------------
        # Write raw sample
        # --------------------------------------------------
        i = self.buffer_index
        self.amplitude_raw[i] = rms

        self.buffer_index = (i + 1) % BUFFER_SIZE
        if self.sample_count < BUFFER_SIZE:
            self.sample_count += 1

        # --------------------------------------------------
        # Normalize using true rolling window
        # --------------------------------------------------
        window_n = min(self.sample_count, NORM_WINDOW)

        (min_amp, max_amp) = self._compute_window_minmax(window_n)

        def normalize(v, vmin, vmax, default):
            rng = vmax - vmin
            if rng < 1e-6:
                return default
            return (v - vmin) / rng

        wrote_idx = (self.buffer_index - 1) % BUFFER_SIZE

        self.amplitude_norm[wrote_idx] = normalize(rms, min_amp, max_amp, 0.0)

        # Color based on amplitude (similar to waveform.rs color scheme)
        amp = self.amplitude_norm[wrote_idx]
        
        if amp < 0.15:
            # Very quiet - white/gray
            brightness = int(180 + amp / 0.15 * 75)
            self.colors[wrote_idx] = (brightness, brightness, brightness, 255)
        elif amp < 0.4:
            # Quiet to medium - white to cyan
            t = (amp - 0.15) / 0.25
            r = int(255 - t * 155)
            g = 255
            b = 255
            self.colors[wrote_idx] = (r, g, b, 255)
        elif amp < 0.65:
            # Medium to loud - cyan to green
            t = (amp - 0.4) / 0.25
            r = int(100 - t * 100)
            g = 255
            b = int(255 - t * 155)
            self.colors[wrote_idx] = (r, g, b, 255)
        elif amp < 0.85:
            # Loud - green to yellow
            t = (amp - 0.65) / 0.2
            r = int(t * 255)
            g = 255
            b = 0
            self.colors[wrote_idx] = (r, g, b, 255)
        else:
            # Very loud - yellow to red
            t = (amp - 0.85) / 0.15
            r = 255
            g = int(255 - t * 100)
            b = 0
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
                (BASELINE_LENGTH + self.amplitude_norm[buf_idx] * MAX_EXTRA_LENGTH)
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
    xos.print("Microphone Waveform — gated start")
    game = MicrophoneWaveform()
    game.run()

