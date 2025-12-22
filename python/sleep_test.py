import xos

print("Starting sleep test...")
print("This should not freeze the UI!")

for i in range(5):
    print(f"Count: {i+1}")

    # NOTE: time.sleep() should also work just fine.
    xos.sleep(1.0)
    
print("Sleep test complete!")

