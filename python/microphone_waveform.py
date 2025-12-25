import xos

# Configuration
NUM_LINES = 512
BASELINE_LENGTH = 0.012
MAX_EXTRA_LENGTH = 0.678
LINE_THICKNESS = 0.003
AMPLIFICATION_FACTOR = 50.0
PROPAGATION_TIME_SECS = 1.0
TARGET_FPS = 60.0


class MicrophoneWaveform(xos.Application):
    def __init__(self):
        super().__init__()
        self.microphone = None

        # Buffers
        self.sample_buffer = [0.0] * NUM_LINES
        self.color_buffer = [(128, 128, 128, 255)] * NUM_LINES
        self.buffer_index = 0
        self.lines_to_add = 0.0

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
        """Non-linear amplification: boosts quiet sounds more than loud ones."""
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
        
        # Clear background
        xos.rasterizer.fill(self.frame, (8, 10, 15, 255))
        
        # Calculate how many lines to add per frame for smooth animation
        lines_per_frame = NUM_LINES / (PROPAGATION_TIME_SECS * TARGET_FPS)
        self.lines_to_add += lines_per_frame
        
        # Only process whole lines (round down)
        lines_to_process = int(self.lines_to_add)
        self.lines_to_add -= lines_to_process
        
        # Clamp to max 20 lines per frame to prevent lag spikes
        lines_to_process = min(lines_to_process, 20)
        
        if lines_to_process == 0:
            self._render(width, height)
            return
        
        # Process the requested number of lines this frame
        for _ in range(lines_to_process):
            # Get batch of samples (one batch = one line)
            batch = self.microphone.get_batch(512)
            if not batch:
                break
            
            # Calculate RMS amplitude
            sum_squares = sum(s * s for s in batch)
            rms = (sum_squares / len(batch)) ** 0.5
            
            # Amplify and normalize
            amplified = self.amplify_nonlinear(rms)
            normalized = max(0.0, min(1.0, amplified))
            
            # Store in circular buffer
            self.sample_buffer[self.buffer_index] = normalized
            
            # Compute color based on amplitude
            amp = normalized
            
            if amp < 0.15:
                brightness = int(180 + amp / 0.15 * 75)
                color = (brightness, brightness, brightness, 255)
            elif amp < 0.4:
                t = (amp - 0.15) / 0.25
                r = int(255 - t * 155)
                g = 255
                b = 255
                color = (r, g, b, 255)
            elif amp < 0.65:
                t = (amp - 0.4) / 0.25
                r = int(100 - t * 100)
                g = 255
                b = int(255 - t * 155)
                color = (r, g, b, 255)
            elif amp < 0.85:
                t = (amp - 0.65) / 0.2
                r = int(t * 255)
                g = 255
                b = 0
                color = (r, g, b, 255)
            else:
                t = (amp - 0.85) / 0.15
                r = 255
                g = int(255 - t * 100)
                b = 0
                color = (r, g, b, 255)
            
            self.color_buffer[self.buffer_index] = color
            self.buffer_index = (self.buffer_index + 1) % NUM_LINES
        
        # Render
        self._render(width, height)
    
    def _render(self, width, height):
        """Render flowing horizontal lines - vectorized"""
        center_x = width * 0.5
        spacing = height / NUM_LINES
        thickness_px = LINE_THICKNESS * height
        
        # Pre-allocate arrays for vectorized rendering
        start_points = []
        end_points = []
        thicknesses = []
        colors = []
        
        for line_idx in range(NUM_LINES):
            buf_idx = (self.buffer_index + line_idx) % NUM_LINES
            amp = self.sample_buffer[buf_idx]
            
            half_len = (BASELINE_LENGTH + amp * MAX_EXTRA_LENGTH) * width * 0.5
            y = height - (line_idx * spacing)
            
            x0 = center_x - half_len
            x1 = center_x + half_len
            
            start_points.append((x0, y))
            end_points.append((x1, y))
            thicknesses.append(thickness_px)
            colors.append(self.color_buffer[buf_idx])
        
        # Single vectorized call for all lines
        xos.rasterizer.lines_batched(
            self.frame,
            start_points,
            end_points,
            thicknesses,
            colors
        )


if __name__ == "__main__":
    xos.print("Microphone Waveform")
    game = MicrophoneWaveform()
    game.run()
