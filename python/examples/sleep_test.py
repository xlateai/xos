import xos

print("Starting sleep test...")

for i in range(10):
    print(f"Count: {i+1}")
    xos.sleep(0.5)

print("Sleep test complete!")

