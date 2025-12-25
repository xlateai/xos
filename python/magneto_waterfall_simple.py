import xos

# =========================
# Configuration
# =========================
FFT_SIZE = 128                      # FFT size (gives 64 frequency bins)
NUM_FREQUENCY_BINS = 64             # Number of vertical pixels (one per frequency bin)
MAGNITUDE_SCALE = 100.0             # Scale up FFT magnitudes


class MagnetometerWaterfall(xos.Application):
    def __init__(self):
        super().__init__()
        self.magnetometer = None
        
        # Sample buffer for FFT
        self.sample_buffer = [0.0] * FFT_SIZE
        self.sample_index = 0
        
        # Baseline for DC removal
        self.baseline = None
        
        # Current FFT magnitudes (one per frequency bin)
        self.current_magnitudes = [0.0] * NUM_FREQUENCY_BINS
        
        # Normalization
        self.min_val = 0.0
        self.max_val = 10.0
        
        # Debug
        self.fft_count = 0
        self.tick_count = 0
        
    def setup(self):
        self.magnetometer = xos.sensors.magnetometer()
        print("🧲📊 Magnetometer Waterfall - Simple single column test")
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
        except:
            print("⚠️  FFT failed")
            return [0.0] * NUM_FREQUENCY_BINS
    
    def magnitude_to_color(self, magnitude):
        """Convert magnitude to color"""
        # Simple normalization
        norm = magnitude / self.max_val
        norm = max(0.0, min(1.0, norm))
        
        # Hot colormap: black → blue → cyan → yellow → white
        if norm < 0.25:
            t = norm / 0.25
            return (0, 0, int(t * 255), 255)
        elif norm < 0.5:
            t = (norm - 0.25) / 0.25
            return (0, int(t * 255), 255, 255)
        elif norm < 0.75:
            t = (norm - 0.5) / 0.25
            return (int(t * 255), 255, int(255 * (1.0 - t)), 255)
        else:
            t = (norm - 0.75) / 0.25
            return (255, 255, int(t * 255), 255)
    
    def tick(self):
        width = self.get_width()
        height = self.get_height()
        
        self.tick_count += 1
        
        # Fill black
        xos.rasterizer.fill(self.frame, (0, 0, 0, 255))
        
        # Read magnetometer
        mx, my, mz = self.magnetometer.read()
        mag = xos.math.sqrt(mx * mx + my * my + mz * mz)
        
        # Initialize baseline
        if self.baseline is None:
            self.baseline = mag
            print(f"📊 Baseline: {mag:.6f}")
            return
        
        # DC removal
        self.baseline += (mag - self.baseline) * 0.01
        signal = mag - self.baseline
        
        # Add to buffer
        self.sample_buffer[self.sample_index] = signal
        self.sample_index += 1
        
        # When buffer full, compute FFT
        if self.sample_index >= FFT_SIZE:
            self.sample_index = 0
            self.fft_count += 1
            
            print(f"📊 Computing FFT #{self.fft_count}...")
            
            # Compute FFT
            windowed = self.apply_window(self.sample_buffer)
            self.current_magnitudes = self.compute_fft_magnitude(windowed)
            
            # Update max for normalization
            max_mag = max(self.current_magnitudes)
            if max_mag > self.max_val:
                self.max_val = max_mag
            
            print(f"   Max magnitude: {max_mag:.4f}, norm_max: {self.max_val:.4f}")
        
        # Draw ONE vertical column at x=100 with frequency bins
        x = 100
        bin_height = height / NUM_FREQUENCY_BINS
        
        for bin_idx in range(NUM_FREQUENCY_BINS):
            mag = self.current_magnitudes[bin_idx]
            color = self.magnitude_to_color(mag)
            
            # Y position: flip so low freq at bottom, high at top
            y = height - (bin_idx + 1) * bin_height
            
            y1 = int(y)
            y2 = int(y + bin_height + 0.5)
            if y2 <= y1:
                y2 = y1 + 1
            
            # Draw a 10-pixel wide column
            xos.rasterizer.rect_filled(
                self.frame,
                x, y1, x + 10, y2,
                color
            )
        
        # Debug every 60 ticks
        if self.tick_count % 60 == 0:
            print(f"🔄 Tick {self.tick_count}: {self.fft_count} FFTs, samples={self.sample_index}/{FFT_SIZE}")
        
        # Draw test markers
        # Red square in top-left
        xos.rasterizer.rect_filled(self.frame, 10, 10, 30, 30, (255, 0, 0, 255))
        # Green square in bottom-left  
        xos.rasterizer.rect_filled(self.frame, 10, height - 30, 30, height - 10, (0, 255, 0, 255))
        # White line where the frequency column is
        xos.rasterizer.rect_filled(self.frame, x - 2, 0, x - 1, height, (255, 255, 255, 255))


if __name__ == "__main__":
    print("🧲📊 Magnetometer Waterfall - Simple Column Test")
    app = MagnetometerWaterfall()
    app.run()

