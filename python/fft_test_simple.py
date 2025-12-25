import xos

# Use standard print (xos.print is now an alias)
print("🔍 FFT Simple Test - Step by step")

# Step 1: Create samples
print("Step 1: Creating samples...")
samples = []
for i in range(8):
    samples.append(float(i))
print(f"  Samples created: {len(samples)} items")
print(f"  First sample: {samples[0]}")

# Step 2: Try FFT
print("\nStep 2: Calling xos.math.fft()...")
try:
    result = xos.math.fft(samples)
    print(f"  FFT returned: {type(result)}")
    print(f"  Result length: {len(result)}")
    
    # Step 3: Unpack result
    print("\nStep 3: Unpacking result...")
    real, imag = result
    print(f"  Real type: {type(real)}")
    print(f"  Imag type: {type(imag)}")
    print(f"  Real length: {len(real)}")
    print(f"  Imag length: {len(imag)}")
    
    # Step 4: Access elements
    print("\nStep 4: Accessing first element...")
    print(f"  real[0] = {real[0]}")
    print(f"  imag[0] = {imag[0]}")
    
    print("\n✅ All steps passed!")
    
except Exception as e:
    print(f"\n❌ Error: {e}")
    print(f"   Error type: {type(e)}")

