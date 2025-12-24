import xos

array = xos.zeros((4, 4))

print(type(array))
print(array)

ones_array = xos.ones((4, 4))
print(type(ones_array))
print(ones_array)


ones_array_dtype = xos.ones((4, 4), dtype=xos.int32)
print(ones_array_dtype)
