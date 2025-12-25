import xos

# List all files and directories
print("=== Files and Directories ===")
xos.data.list()

# Read the first 8 lines from this file (data_explore_test.py)
print("\n=== First 8 lines of data_explore_test.py ===")
for i, line in enumerate(xos.data.read_lines("data_explore_test.py", start=0, end=8)):
    print(f"{i}: {line}")

