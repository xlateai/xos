import xos

# Configuration - matching waveform.rs but optimized for Python performance
NUM_LINES = 512
BASELINE_LENGTH = 0.012  # 20% of original (0.06)
MAX_EXTRA_LENGTH = 0.678  # 1.5x total of original (0.46) minus baseline
LINE_THICKNESS = 0.003
PROPAGATION_TIME_SECS = 1.0  # Time for a line to travel from top to bottom
AMPLIFICATION_FACTOR = 50.0  # Multiply raw audio by this amount before compression
SAMPLE_RATE = 44100.0  # Expected sample rate
TARGET_FPS = 60.0  # Target frame rate for smooth animation

EPSILON = 1e-6  # threshold for "non-zero" audio signal


class MicrophoneWaveform(xos.Application):
    def __init__(self):
        super().__init__()
        self.microphone = None

        # Buffers matching Rust implementation
        self.sample_buffer = [0.0] * NUM_LINES
        self.color_buffer = [(128, 128, 128, 255)] * NUM_LINES
        self.buffer_index = 0
        
        # For gated start
        self.waiting_for_signal = True

    def setup(self):
        # Print available input devices
        devices = xos.audio.get_input_devices()
        if devices is None:
            xos.print("ERROR: get_input_devices() returned None")
            raise RuntimeError("Failed to get audio devices")
        
        if not devices:
            xos.print("WARNING: No audio input devices found!")
            raise RuntimeError("No audio input devices available")
        
        xos.print(f"Available audio input devices ({len(devices)} found):")
        for dev in devices:
            xos.print(f"  [{dev['id']}] {dev['name']}")
        
        # Check system type - only use dialoguer on non-iOS systems
        system_type = xos.system.get_system_type()
        
        if system_type == "IOS":
            # On iOS, use the first available microphone (no dialoguer)
            device_id = 0
            xos.print(f"🔊 Using device: {devices[device_id]['name']}")
        else:
            # On macOS/Linux/Windows, let user select with dialoguer
            device_names = [dev['name'] for dev in devices]
            device_id = xos.dialoguer.select("Select microphone", device_names, default=0)
            xos.print(f"🔊 Selected device: {devices[device_id]['name']}")
        
        # Initialize microphone with selected device
        try:
            self.microphone = xos.audio.Microphone(device_id=device_id, buffer_duration=1.0)
            xos.print("Microphone Waveform initialized")
        except Exception as e:
            xos.print(f"Failed to initialize microphone: {e}")
            raise

    def amplify_nonlinear(self, value):
        """
        Non-linear amplification: boosts quiet sounds more than loud ones.
        Multiplies by AMPLIFICATION_FACTOR but compresses the high end with logarithmic decay.
        EXACT match to Rust implementation.
        """
        abs_val = abs(value)
        
        # Multiply by AMPLIFICATION_FACTOR first
        boosted = abs_val * AMPLIFICATION_FACTOR
        
        # Apply logarithmic compression to prevent clipping and show lower volumes better
        # This heavily compresses the top end while preserving low-end dynamics
        if boosted < 0.1:
            # Very quiet - linear boost
            amplified = boosted * 2.0
        elif boosted < 1.0:
            # Quiet - gentle compression
            amplified = 0.2 + (boosted - 0.1) * 1.5
        else:
            # Loud - logarithmic decay to compress the top end
            # This makes the scale: 0-1 maps to 0-1.55, but 10 maps to ~2.8
            # So we can see quiet sounds while loud sounds don't dominate
            import math
            amplified = 0.2 + 1.35 + max(0.0, math.log(boosted - 1.0)) * 0.4
        
        return -amplified if value < 0.0 else amplified

    def tick(self):
        if self.microphone is None:
            return
        
        width = self.get_width()
        height = self.get_height()
        
        # Get a batch of samples from the microphone (optimized batch size)
        batch = self.microphone.get_batch(512)
        
        if not batch:
            self._render(width, height)
            return
        
        # Calculate RMS (root mean square) amplitude from the batch
        sum_squares = sum(s * s for s in batch)
        rms = (sum_squares / len(batch)) ** 0.5 if batch else 0.0
        
        # --------------------------------------------------
        # WAIT until first non-zero audio signal (gated start)
        # --------------------------------------------------
        if self.waiting_for_signal:
            if rms < EPSILON:
                return  # do nothing yet
            else:
                self.waiting_for_signal = False
                xos.print("🎤 Microphone active — starting waveform")
        
        # Amplify non-linearly
        amplified = self.amplify_nonlinear(rms)
        normalized = max(0.0, min(1.0, amplified))
        
        # Store in circular buffer
        self.sample_buffer[self.buffer_index] = normalized
        
        # Compute color based on amplitude - EXACT match to Rust
        amp = normalized
        
        if amp < 0.15:
            # Very quiet - white/gray
            brightness = int(180 + amp / 0.15 * 75)
            color = (brightness, brightness, brightness, 255)
        elif amp < 0.4:
            # Quiet to medium - white to cyan
            t = (amp - 0.15) / 0.25
            r = int(255 - t * 155)
            g = 255
            b = 255
            color = (r, g, b, 255)
        elif amp < 0.65:
            # Medium to loud - cyan to green
            t = (amp - 0.4) / 0.25
            r = int(100 - t * 100)
            g = 255
            b = int(255 - t * 155)
            color = (r, g, b, 255)
        elif amp < 0.85:
            # Loud - green to yellow
            t = (amp - 0.65) / 0.2
            r = int(t * 255)
            g = 255
            b = 0
            color = (r, g, b, 255)
        else:
            # Very loud - yellow to red
            t = (amp - 0.85) / 0.15
            r = 255
            g = int(255 - t * 100)
            b = 0
            color = (r, g, b, 255)
        
        self.color_buffer[self.buffer_index] = color
        self.buffer_index = (self.buffer_index + 1) % NUM_LINES
        
        # Render the waveform
        self._render(width, height)
    
    def _render(self, width, height):
        """Render flowing horizontal lines (like magnetometer)"""
        center_x = width * 0.5
        spacing = height / NUM_LINES
        thickness_px = LINE_THICKNESS * height
        
        for line_idx in range(NUM_LINES):
            buf_idx = (self.buffer_index + line_idx) % NUM_LINES
            amp = self.sample_buffer[buf_idx]
            
            # Calculate line length (baseline + extra based on amplitude)
            half_len = (BASELINE_LENGTH + amp * MAX_EXTRA_LENGTH) * width * 0.5
            
            # Y position flows from top to bottom
            y = height - (line_idx * spacing)
            
            x0 = center_x - half_len
            x1 = center_x + half_len
            
            color = self.color_buffer[buf_idx]
            
            xos.rasterizer.lines(
                self.frame,
                [(x0, y)],
                [(x1, y)],
                [thickness_px],
                color
            )


if __name__ == "__main__":
    xos.print("Microphone Waveform — 60fps smooth animation")
    game = MicrophoneWaveform()
    game.run()
