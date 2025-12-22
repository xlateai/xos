import time

print("Starting sleep test...")
print("This should not freeze the UI!")
print("Click the red STOP button to stop early!\n")

for i in range(20):
    print(f"Count: {i+1}")
    time.sleep(1.0)
    
print("\nSleep test complete!")

