import xos

# =========================
# Configuration
# =========================
FFT_SIZE = 128                  # Number of samples for FFT (gives 64 frequency bins)
SAMPLE_RATE = 100               # Approximate Hz (adjust based on actual tick rate)

# Color mapping
MIN_DB = -60.0                  # Minimum dB for color scale
MAX_DB = 0.0                    # Maximum dB for color scale

# Rolling normalization
PERCENTILE_HIGH = 0.95          # For adaptive max
PERCENTILE_LOW = 0.05           # For adaptive min

# History buffer for adaptive normalization
NORM_HISTORY_SIZE = 50

# Magnitude scaling
MAGNITUDE_SCALE = 100.0         # Scale up FFT magnitudes for better visibility


class MagnetometerWaterfall(xos.Application):
    def __init__(self):
        super().__init__()
        self.magnetometer = None
        
        # Sample accumulation buffer for FFT - use xos array
        self.sample_buffer = xos.zeros((FFT_SIZE,))
        self.sample_index = 0
        
        # Baseline for DC removal
        self.baseline = None
        
        # Waterfall data: List of column vectors (each is FFT result)
        self.waterfall_columns = []
        self.max_columns = 512
        
        # Adaptive normalization
        self.norm_history = []
        self.min_val = 0.0
        self.max_val = 10.0
        
        # Debug
        self.fft_count = 0
        
    def setup(self):
        self.magnetometer = xos.sensors.magnetometer()
        print("🧲📊 Magnetometer Waterfall — frequency spectrum visualization")
        print(f"Config: FFT_SIZE={FFT_SIZE}, frequency_bins={FFT_SIZE//2}")
    
    def percentile(self, data, p):
        """Compute percentile of data"""
        if not data:
            return 0.0
        s = sorted(data)
        idx = int(len(s) * p)
        return s[min(idx, len(s) - 1)]
    
    def compute_fft_magnitude(self, samples):
        """
        Compute FFT magnitude spectrum from real samples using xos.math.fft.
        Returns frequency bin magnitudes (first half of FFT).
        """
        try:
            # Use fast FFT implementation
            fft_result = xos.math.fft(samples)
            real_parts = fft_result[0]
            imag_parts = fft_result[1]
            
            # Compute magnitudes for first half (positive frequencies)
            N = len(samples)
            half_n = N // 2
            magnitudes = []
            
            for i in range(half_n):
                r = real_parts[i]
                im = imag_parts[i]
                magnitude = xos.math.sqrt(r * r + im * im)
                magnitudes.append(magnitude)
            
            return magnitudes
        except:
            # If FFT fails, return zeros
            print("⚠️  FFT computation failed, returning zeros")
            return [0.0] * (len(samples) // 2)
    
    def apply_window(self, samples):
        """Apply Hamming window to reduce spectral leakage - vectorized"""
        N = len(samples)
        windowed = []
        for i in range(N):
            w = 0.54 - 0.46 * xos.math.cos(2.0 * 3.14159265359 * i / (N - 1))
            windowed.append(samples[i] * w)
        return windowed
    
    def magnitude_to_color(self, magnitude, min_val, max_val):
        """Convert magnitude to color using a hot/plasma-like colormap"""
        # Normalize to 0-1 range
        if max_val - min_val < 1e-9:
            norm = magnitude * 10.0
            norm = max(0.0, min(1.0, norm))
        else:
            norm = (magnitude - min_val) / (max_val - min_val)
            norm = max(0.0, min(1.0, norm))
        
        # Hot colormap: black → blue → cyan → yellow → white
        if norm < 0.25:
            t = norm / 0.25
            r = 0
            g = 0
            b = int(t * 128)
        elif norm < 0.5:
            t = (norm - 0.25) / 0.25
            r = 0
            g = int(t * 200)
            b = 128 + int(t * 127)
        elif norm < 0.75:
            t = (norm - 0.5) / 0.25
            r = int(t * 255)
            g = 200 + int(t * 55)
            b = int(255 * (1.0 - t))
        else:
            t = (norm - 0.75) / 0.25
            r = 255
            g = 255
            b = int(t * 255)
        
        return (r, g, b, 255)
    
    def tick(self):
        width = self.get_width()
        height = self.get_height()
        
        # Clear background to black
        xos.rasterizer.fill(self.frame, (0, 0, 0, 255))
        
        # Read magnetometer
        mx, my, mz = self.magnetometer.read()
        mag = xos.math.sqrt(mx * mx + my * my + mz * mz)
        
        # Initialize baseline
        if self.baseline is None:
            self.baseline = mag
            print(f"📊 Initialized baseline: {mag:.6f}")
            return
        
        # DC removal
        self.baseline += (mag - self.baseline) * 0.01
        signal = mag - self.baseline
        
        # Add to sample buffer
        self.sample_buffer['_data'][self.sample_index] = signal
        self.sample_index += 1
        
        # Debug: print first few samples
        if self.sample_index <= 3:
            print(f"📊 Sample {self.sample_index}: mag={mag:.6f}, baseline={self.baseline:.6f}, signal={signal:.6f}")
        
        # When buffer is full, compute FFT
        if self.sample_index >= FFT_SIZE:
            self.sample_index = 0
            
            print(f"📊 Computing FFT #{self.fft_count + 1}...")
            
            # Get data from xos array
            samples_list = list(self.sample_buffer['_data'])
            
            # Print sample statistics
            max_sample = max(samples_list)
            min_sample = min(samples_list)
            avg_sample = sum(samples_list) / len(samples_list)
            print(f"   Samples: min={min_sample:.6f}, avg={avg_sample:.6f}, max={max_sample:.6f}")
            
            # Apply window and compute FFT
            windowed = self.apply_window(samples_list)
            raw_magnitudes = self.compute_fft_magnitude(windowed)
            
            # Scale magnitudes for visibility
            magnitudes = [m * MAGNITUDE_SCALE for m in raw_magnitudes]
            
            # Print FFT statistics
            max_mag = max(magnitudes) if magnitudes else 0.0
            min_mag = min(magnitudes) if magnitudes else 0.0
            avg_mag = sum(magnitudes) / len(magnitudes) if magnitudes else 0.0
            print(f"   FFT mags: min={min_mag:.6f}, avg={avg_mag:.6f}, max={max_mag:.6f}")
            
            # Add new column to the LEFT (beginning) of waterfall
            self.waterfall_columns.insert(0, magnitudes)
            
            # Keep only max_columns
            if len(self.waterfall_columns) > self.max_columns:
                self.waterfall_columns.pop()
            
            self.fft_count += 1
            
            # Update normalization history
            for mag in magnitudes:
                self.norm_history.append(mag)
            if len(self.norm_history) > NORM_HISTORY_SIZE * (FFT_SIZE // 2):
                self.norm_history = self.norm_history[-(NORM_HISTORY_SIZE * (FFT_SIZE // 2)):]
            
            # Compute adaptive min/max
            if len(self.norm_history) >= 32:
                new_min = self.percentile(self.norm_history, PERCENTILE_LOW)
                new_max = self.percentile(self.norm_history, PERCENTILE_HIGH)
                
                if new_max - new_min > 0.001:
                    self.min_val = new_min
                    self.max_val = new_max
                
                print(f"   Norm range: [{self.min_val:.6f}, {self.max_val:.6f}]")
        
        # Render waterfall
        self.render_waterfall(width, height)
        
        # Draw a progress indicator at the bottom
        progress_width = int((self.sample_index / FFT_SIZE) * width)
        if progress_width > 0:
            xos.rasterizer.rect_filled(
                self.frame,
                0, height - 2, progress_width, height,
                (0, 255, 0, 200)
            )
    
    def render_waterfall(self, width, height):
        """
        Render waterfall plot:
        - X axis (left to right): time, newest on left
        - Y axis (bottom to top): frequency bins, low freq at bottom
        """
        if not self.waterfall_columns:
            print("📊 No FFT data to render yet")
            return
        
        num_bins = FFT_SIZE // 2
        num_columns = min(len(self.waterfall_columns), width)
        
        # Calculate pixel dimensions
        col_width = width / num_columns if num_columns > 0 else 1
        bin_height = height / num_bins
        
        if self.fft_count == 1:
            print(f"📊 Rendering: {num_columns} cols x {num_bins} bins")
            print(f"   col_width={col_width:.2f}, bin_height={bin_height:.2f}")
        
        # Draw each column (time slice)
        pixels_drawn = 0
        for col_idx in range(num_columns):
            if col_idx >= len(self.waterfall_columns):
                break
            
            magnitudes = self.waterfall_columns[col_idx]
            x = col_idx * col_width
            
            # Draw each frequency bin in this column
            for bin_idx in range(num_bins):
                mag = magnitudes[bin_idx]
                color = self.magnitude_to_color(mag, self.min_val, self.max_val)
                
                # Y position: flip so low frequencies at bottom
                y = height - (bin_idx + 1) * bin_height
                
                # Draw filled rectangle for this bin
                x1 = int(x)
                y1 = int(y)
                x2 = int(x + col_width + 0.5)
                y2 = int(y + bin_height + 0.5)
                
                # Ensure valid rectangle
                if x2 <= x1:
                    x2 = x1 + 1
                if y2 <= y1:
                    y2 = y1 + 1
                
                xos.rasterizer.rect_filled(
                    self.frame,
                    x1, y1, x2, y2,
                    color
                )
                pixels_drawn += 1
        
        if self.fft_count == 1:
            print(f"   Drew {pixels_drawn} rectangles")


if __name__ == "__main__":
    print("🧲📊 Magnetometer Waterfall")
    app = MagnetometerWaterfall()
    app.run()
