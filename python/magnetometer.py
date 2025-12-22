import xos

# Magnetometer test script with sleep
# Now runs in background thread - won't freeze UI!

print("=== Magnetometer Test ===")
print("Initializing...")

try:
    mag = xos.sensors.magnetometer.init()
    print("✅ Magnetometer initialized!")
except Exception as e:
    print(f"❌ Error: {e}")
    exit()

# Read every 3 seconds for 15 seconds
print("\nReading magnetometer every 3 seconds...")
print("(UI should remain responsive!)\n")

for i in range(5):
    xos.sleep(3.0)
    
    try:
        readings = mag.drain_readings()
        if readings:
            # Calculate averages
            sum_x = sum(r['x'] for r in readings)
            sum_y = sum(r['y'] for r in readings)
            sum_z = sum(r['z'] for r in readings)
            
            avg_x = sum_x / len(readings)
            avg_y = sum_y / len(readings)
            avg_z = sum_z / len(readings)
            magnitude = (avg_x**2 + avg_y**2 + avg_z**2) ** 0.5
            
            print(f"--- Reading {i+1} ({len(readings)} samples) ---")
            print(f"Avg X: {avg_x:.2f} µT")
            print(f"Avg Y: {avg_y:.2f} µT")
            print(f"Avg Z: {avg_z:.2f} µT")
            print(f"Magnitude: {magnitude:.2f} µT")
        else:
            print(f"--- Reading {i+1} ---")
            print("No new readings")
    except Exception as e:
        print(f"Error: {e}")

# Cleanup
mag.cleanup()
print("\n✅ Done!")
print("Total readings:", mag.get_total_readings())

