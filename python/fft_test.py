import xos

# Test FFT with a simple sine wave
N = 128
samples = []

# Create a sine wave at frequency bin 10
for i in range(N):
    angle = 2.0 * 3.14159265359 * 10.0 * i / N
    samples.append(xos.math.sin(angle))

xos.print(f"Testing FFT with {N} samples (sine wave at bin 10)")
xos.print(f"First 8 samples: {samples[:8]}")

# Compute FFT
real, imag = xos.math.fft(samples)

xos.print(f"FFT complete!")
xos.print(f"First 8 real parts: {real[:8]}")
xos.print(f"First 8 imag parts: {imag[:8]}")

# Compute magnitudes
magnitudes = []
for i in range(N // 2):
    mag = xos.math.sqrt(real[i] * real[i] + imag[i] * imag[i])
    magnitudes.append(mag)

xos.print(f"\nMagnitudes (first 16 bins):")
for i in range(16):
    xos.print(f"  Bin {i}: {magnitudes[i]:.4f}")

xos.print(f"\nPeak at bin {magnitudes.index(max(magnitudes))}: {max(magnitudes):.4f}")
xos.print("✅ FFT test complete!")

