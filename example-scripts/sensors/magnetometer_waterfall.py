import xos

# =========================
# Configuration
# =========================
FFT_SIZE = 128                      # FFT size (must be power of 2) - smaller = faster
NUM_FREQUENCY_BINS = 128             # Number of frequency bins to display
MAGNITUDE_SCALE = 100.0             # Scale for magnetometer magnitudes (after DC removal)


class MagnetometerWaterfall(xos.Application):
    def __init__(self):
        super().__init__()
        
        # Sample buffer for FFT
        self.sample_buffer = [0.0] * FFT_SIZE
        self.sample_index = 0
        
        # DC removal (running average)
        self.dc_average = 0.0
        self.dc_alpha = 0.01  # Smoothing factor for DC tracking
        
        # Waterfall history: list of color rows (pre-computed), newest at index 0
        self.waterfall_history = []
        self.max_history = None  # Will be calculated based on screen size
        
        # Debug
        self.fft_count = 0
        self.tick_count = 0
        self.first_fft_done = False
        
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
        width = self.get_width()
        height = self.get_height()
        
        # Calculate pixel sizes to fill entire screen
        # Horizontal: NUM_FREQUENCY_BINS bins fill full width
        pixel_width = width / NUM_FREQUENCY_BINS
        
        # Vertical: determine how many rows fit, then stretch to fill height
        num_rows_on_screen = int(height / pixel_width)
        pixel_height = height / num_rows_on_screen if num_rows_on_screen > 0 else pixel_width
        
        # Set max history if not set yet
        if self.max_history is None:
            self.max_history = num_rows_on_screen
        
        self.tick_count += 1
        
        # Fill black
        xos.rasterizer.fill(self.frame, (0, 0, 0, 255))
        
        # Read magnetometer
        mag = xos.sensors.magnetometer()
        x, y, z = mag['x'], mag['y'], mag['z']
        
        # Compute magnitude from x, y, z components
        magnitude = xos.math.sqrt(x * x + y * y + z * z)
        
        # Remove DC component (adaptive high-pass filter)
        self.dc_average = self.dc_average * (1.0 - self.dc_alpha) + magnitude * self.dc_alpha
        magnitude_ac = magnitude - self.dc_average
        
        # Add sample to buffer
        if self.sample_index < FFT_SIZE:
            self.sample_buffer[self.sample_index] = magnitude_ac
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
                
                if not self.first_fft_done:
                    print(f"🧲 First FFT! DC avg: {self.dc_average:.2f}, Min mag: {row_min:.4f}, Max mag: {row_max:.4f}")
                    self.first_fft_done = True
                
                # Vectorized: compute all colors at once
                color_row = self.magnitudes_to_colors(magnitudes, row_min, row_max)
                
                # Add color row to history at the beginning (newest)
                self.waterfall_history.insert(0, color_row)
                
                # Trim history to max rows
                if len(self.waterfall_history) > self.max_history:
                    self.waterfall_history.pop()
        
        # Render waterfall
        self.render_waterfall(width, height, pixel_width, pixel_height, num_rows_on_screen)
        
        # Show progress bar while filling buffer (before first FFT)
        if not self.first_fft_done and self.sample_index > 0:
            progress = self.sample_index / FFT_SIZE
            bar_width = int(width * 0.8 * progress)
            bar_height = 10
            bar_x = int(width * 0.1)
            bar_y = height // 2 - bar_height // 2
            xos.rasterizer.rects_filled(self.frame, bar_x, bar_y, bar_x + bar_width, bar_y + bar_height, (0, 255, 0, 255))
    
    def render_waterfall(self, width, height, pixel_width, pixel_height, max_rows):
        """
        Render waterfall - 100% VECTORIZED (NO Python loops):
        - Pass structured data directly to Rust
        - Rust handles all iteration and pixel filling
        - Fills entire screen perfectly (100% width and height)
        """
        if not self.waterfall_history:
            return
        
        # Use all available rows up to max
        num_rows = min(len(self.waterfall_history), max_rows)
        
        # Pass row data + dimensions to Rust kernel
        # Rust will handle ALL iteration internally
        xos.rasterizer.rects_filled(
            self.frame,
            self.waterfall_history[:num_rows],  # List of color rows
            NUM_FREQUENCY_BINS,
            int(pixel_width),
            int(pixel_height),
            num_rows
        )


if __name__ == "__main__":
    print("🧲📊 Magnetometer Waterfall")
    app = MagnetometerWaterfall()
    app.run()

