import xos

# List all files and directories
print("=== Files and Directories ===")
files = xos.data.list()
print(files)

# Find the first .py file
python_files = [f for f in files if f.endswith('.py') and not f.endswith('/')]
if python_files:
    first_py_file = python_files[0]
    print(f"\n=== First 8 lines of {first_py_file} ===")
    for i, line in enumerate(xos.data.read_lines(first_py_file, start=0, end=8)):
        print(f"{i}: {line}")
else:
    print("\nNo Python files found!")

