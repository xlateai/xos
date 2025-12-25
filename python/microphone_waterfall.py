import xos

# =========================
# Configuration
# =========================
FFT_SIZE = 128                      # FFT size (must be power of 2) - smaller = faster
NUM_FREQUENCY_BINS = 128             # Number of frequency bins to display
MAGNITUDE_SCALE = 1.0               # Scale for audio magnitudes


class MicrophoneWaterfall(xos.Application):
    def __init__(self):
        super().__init__()
        self.microphone = None
        
        # Sample buffer for FFT
        self.sample_buffer = [0.0] * FFT_SIZE
        self.sample_index = 0
        
        # Waterfall history: list of color rows (pre-computed), newest at index 0
        self.waterfall_history = []
        self.max_history = None  # Will be calculated based on screen size
        
        # Debug
        self.fft_count = 0
        self.tick_count = 0
        
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
        
        self.microphone = xos.audio.Microphone(
            device_id=device_id,
            buffer_duration=0.05
        )
        
        print("🎤📊 Microphone Waterfall - Square pixels")
        print(f"FFT_SIZE={FFT_SIZE}, NUM_FREQUENCY_BINS={NUM_FREQUENCY_BINS}")
    
    def apply_window(self, samples):
        """Apply Hamming window"""
        N = len(samples)
        windowed = []
        for i in range(N):
            w = 0.54 - 0.46 * xos.math.cos(2.0 * 3.14159265359 * i / (N - 1))
            windowed.append(samples[i] * w)
        return windowed
    
    def compute_fft_magnitude(self, samples):
        """Compute FFT and return magnitudes"""
        try:
            real_parts, imag_parts = xos.math.fft(samples)
            
            magnitudes = []
            for i in range(NUM_FREQUENCY_BINS):
                r = real_parts[i]
                im = imag_parts[i]
                magnitude = xos.math.sqrt(r * r + im * im)
                magnitudes.append(magnitude * MAGNITUDE_SCALE)
            
            return magnitudes
        except Exception as e:
            print(f"⚠️  FFT failed: {e}")
            return [0.0] * NUM_FREQUENCY_BINS
    
    def magnitudes_to_colors(self, magnitudes, min_val, max_val):
        """Vectorized: Convert all magnitudes to colors at once"""
        # Normalize all magnitudes at once
        if max_val - min_val < 1e-9:
            norms = [0.5] * len(magnitudes)
        else:
            norms = [(max(0.0, min(1.0, (m - min_val) / (max_val - min_val)))) for m in magnitudes]
        
        # Vectorized hot colormap computation
        colors = []
        for norm in norms:
            if norm < 0.25:
                t = norm / 0.25
                colors.append((0, 0, int(t * 255), 255))
            elif norm < 0.5:
                t = (norm - 0.25) / 0.25
                colors.append((0, int(t * 255), 255, 255))
            elif norm < 0.75:
                t = (norm - 0.5) / 0.25
                colors.append((int(t * 255), 255, int(255 * (1.0 - t)), 255))
            else:
                t = (norm - 0.75) / 0.25
                colors.append((255, 255, int(t * 255), 255))
        
        return colors
    
    def tick(self):
        if self.microphone is None:
            return
            
        width = self.get_width()
        height = self.get_height()
        
        # Calculate pixel size for square pixels
        # NUM_FREQUENCY_BINS pixels stretched across width
        pixel_size = width / NUM_FREQUENCY_BINS
        
        # Calculate how many rows fit on screen (square pixels!)
        num_rows_on_screen = int(height / pixel_size)
        
        # Set max history if not set yet
        if self.max_history is None:
            self.max_history = num_rows_on_screen
            print(f"📐 Screen: {width}x{height}, Pixel size: {pixel_size:.1f}, Max rows: {num_rows_on_screen}")
        
        self.tick_count += 1
        
        # Fill black
        xos.rasterizer.fill(self.frame, (0, 0, 0, 255))
        
        # Get audio samples
        batch = self.microphone.get_batch(256)
        if not batch or not batch['_data']:
            return
        
        audio_samples = batch['_data']
        
        # Fill our FFT buffer from audio samples
        for sample in audio_samples:
            if self.sample_index < FFT_SIZE:
                self.sample_buffer[self.sample_index] = sample
                self.sample_index += 1
                
                # When buffer full, compute FFT
                if self.sample_index >= FFT_SIZE:
                    self.sample_index = 0
                    self.fft_count += 1
                    
                    # Compute FFT
                    windowed = self.apply_window(self.sample_buffer)
                    magnitudes = self.compute_fft_magnitude(windowed)
                    
                    # Per-row normalization: each row uses its OWN min/max
                    row_min = min(magnitudes)
                    row_max = max(magnitudes)
                    
                    # Vectorized: compute all colors at once
                    color_row = self.magnitudes_to_colors(magnitudes, row_min, row_max)
                    
                    # Add color row to history at the beginning (newest)
                    self.waterfall_history.insert(0, color_row)
                    
                    # Trim history to max rows
                    if len(self.waterfall_history) > self.max_history:
                        self.waterfall_history.pop()
                    
                    if self.fft_count % 20 == 0:
                        print(f"📊 FFT #{self.fft_count}: Min={row_min:.4f}, Max={row_max:.4f}, History={len(self.waterfall_history)}/{self.max_history} rows")
        
        # Render waterfall
        self.render_waterfall(width, height, pixel_size)
    
    def render_waterfall(self, width, height, pixel_size):
        """
        Render waterfall - 100% VECTORIZED (NO Python loops):
        - Pass structured data directly to Rust
        - Rust handles all iteration and pixel filling
        """
        if not self.waterfall_history:
            return
        
        # Calculate dimensions
        num_rows = min(len(self.waterfall_history), int(height / pixel_size))
        pixel_size_int = int(pixel_size)
        
        # Pass row data + dimensions to Rust kernel
        # Rust will handle ALL iteration internally
        xos.rasterizer.rects_filled(
            self.frame,
            self.waterfall_history[:num_rows],  # List of color rows
            NUM_FREQUENCY_BINS,
            pixel_size_int,
            num_rows
        )


if __name__ == "__main__":
    print("🎤📊 Microphone Waterfall")
    app = MicrophoneWaterfall()
    app.run()
