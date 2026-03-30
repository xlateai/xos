import xos

# Simple API: just create and read
magnetometer = xos.sensors.magnetometer()

for i in range(10):
    x, y, z = magnetometer.read()
    magnitude = (x**2 + y**2 + z**2) ** 0.5
    
    print(f"Reading {i+1}:")
    print(f"  X: {x:.2f} µT")
    print(f"  Y: {y:.2f} µT")
    print(f"  Z: {z:.2f} µT")
    print(f"  Magnitude: {magnitude:.2f} µT\n")
    
    xos.sleep(3.0)

print("✅ Done!")
