#[cfg(any(target_os = "macos", target_os = "ios"))]
use metal;

/// Device where array data resides
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    /// CPU memory
    Cpu,
    /// Metal GPU (macOS/iOS only)
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    Metal,
}

/// Internal storage for array data - can be CPU or Metal GPU memory
#[derive(Debug)]
enum Storage<T> {
    /// CPU memory storage
    Cpu(Vec<T>),
    /// Metal GPU buffer storage (macOS/iOS only)
    /// Stores the buffer and the number of elements (for length calculations)
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    Metal {
        buffer: metal::Buffer,
        len: usize,
    },
}

impl Device {
    /// Get the default device (const function for use in const contexts)
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    pub const fn default() -> Self {
        Device::Metal  // Default to Metal on Apple platforms
    }
    
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    pub const fn default() -> Self {
        Device::Cpu  // Default to CPU on other platforms
    }
}

impl Default for Device {
    fn default() -> Self {
        Self::default()
    }
}

/// A typed array of numbers with a shape and device
#[derive(Debug)]
pub struct Array<T> {
    /// The storage (CPU or Metal GPU)
    storage: Storage<T>,
    /// The shape of the array (dimensions)
    shape: Vec<usize>,
    /// The device where this array resides
    device: Device,
}

impl<T> Array<T> {
    /// Create a new array from data and shape on the default device
    pub fn new(data: Vec<T>, shape: Vec<usize>) -> Self 
    where
        T: Copy,
    {
        Self::new_on_device(data, shape, Device::default())
    }

    /// Create a new array from data and shape on a specific device
    pub fn new_on_device(data: Vec<T>, shape: Vec<usize>, device: Device) -> Self 
    where
        T: Copy,
    {
        let expected_len: usize = shape.iter().product();
        assert_eq!(
            data.len(),
            expected_len,
            "Data length {} does not match shape {:?} (expected {})",
            data.len(),
            shape,
            expected_len
        );
        
        let storage = match device {
            Device::Cpu => Storage::Cpu(data),
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            Device::Metal => {
                // Get the Metal device and create a buffer directly
                let metal_device = metal::Device::system_default()
                    .expect("No Metal device available");
                let data_len_bytes = (data.len() * std::mem::size_of::<T>()) as u64;
                let buffer = metal_device.new_buffer_with_data(
                    data.as_ptr() as *const _,
                    data_len_bytes,
                    metal::MTLResourceOptions::StorageModeShared,
                );
                Storage::Metal {
                    buffer,
                    len: data.len(),
                }
            }
        };
        
        Self {
            storage,
            shape,
            device,
        }
    }

    /// Get the device where this array resides
    pub fn device(&self) -> Device {
        self.device
    }

    /// Move array to a different device
    pub fn to_device(&self, target_device: Device) -> Self 
    where
        T: Clone + Copy + Default,
    {
        if self.device == target_device {
            // Same device - for CPU we can clone, for Metal we'd need buffer copy (not implemented)
            match &self.storage {
                Storage::Cpu(vec) => {
                    return Self::new_on_device(vec.clone(), self.shape.clone(), target_device);
                }
                #[cfg(any(target_os = "macos", target_os = "ios"))]
                Storage::Metal { .. } => {
                    // For now, read back to CPU then create new Metal buffer
                    // TODO: Implement proper Metal buffer copying
                    let cpu_data = self.to_device(Device::Cpu);
                    return cpu_data.to_device(target_device);
                }
            }
        }
        
        // Transfer between devices
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            match (&self.storage, target_device) {
                (Storage::Cpu(vec), Device::Cpu) => {
                    Self::new_on_device(vec.clone(), self.shape.clone(), target_device)
                }
                (Storage::Cpu(vec), Device::Metal) => {
                    Self::new_on_device(vec.clone(), self.shape.clone(), target_device)
                }
                (Storage::Metal { buffer, len }, Device::Cpu) => {
                    // Read back from Metal to CPU
                    let mut cpu_data = vec![T::default(); *len];
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            buffer.contents() as *const T,
                            cpu_data.as_mut_ptr(),
                            *len,
                        );
                    }
                    Self::new_on_device(cpu_data, self.shape.clone(), target_device)
                }
                (Storage::Metal { .. }, Device::Metal) => {
                    // Metal to Metal - read to CPU first, then back to Metal
                    let cpu_data = self.to_device(Device::Cpu);
                    cpu_data.to_device(target_device)
                }
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        {
            match (&self.storage, target_device) {
                (Storage::Cpu(vec), Device::Cpu) => {
                    Self::new_on_device(vec.clone(), self.shape.clone(), target_device)
                }
                (Storage::Cpu(_), Device::Metal) => {
                    panic!("Metal device not available on this platform");
                }
            }
        }
    }

    /// Get the shape of the array
    pub fn shape(&self) -> &[usize] {
        &self.shape
    }

    /// Get the data as a slice
    /// Note: For Metal arrays, this will panic. Use metal_buffer() for Metal arrays.
    pub fn data(&self) -> &[T] {
        match &self.storage {
            Storage::Cpu(vec) => vec.as_slice(),
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            Storage::Metal { .. } => {
                panic!("Cannot get CPU slice from Metal array. Use metal_buffer() instead.");
            }
        }
    }

    /// Get mutable access to the data
    /// Note: For Metal arrays, this will panic. Metal arrays are immutable from CPU side.
    pub fn data_mut(&mut self) -> &mut [T] {
        match &mut self.storage {
            Storage::Cpu(vec) => vec.as_mut_slice(),
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            Storage::Metal { .. } => {
                panic!("Cannot get mutable CPU slice from Metal array. Metal arrays are GPU-resident.");
            }
        }
    }
    
    /// Get the Metal buffer (only valid for Metal arrays)
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    pub(crate) fn metal_buffer(&self) -> &metal::Buffer {
        match &self.storage {
            Storage::Metal { buffer, .. } => buffer,
            Storage::Cpu(_) => {
                panic!("metal_buffer() called on CPU array");
            }
        }
    }

    /// Get the number of dimensions
    pub fn ndim(&self) -> usize {
        self.shape.len()
    }

    /// Get the total number of elements
    pub fn len(&self) -> usize {
        match &self.storage {
            Storage::Cpu(vec) => vec.len(),
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            Storage::Metal { len, .. } => *len,
        }
    }

    /// Check if the array is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get an element by flat index
    /// Note: For Metal arrays, this will panic. Metal arrays should be accessed via GPU kernels.
    pub fn get(&self, index: usize) -> Option<&T> {
        match &self.storage {
            Storage::Cpu(vec) => vec.get(index),
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            Storage::Metal { .. } => {
                panic!("Cannot get element from Metal array. Use GPU kernels to access Metal data.");
            }
        }
    }

    /// Get a mutable element by flat index
    /// Note: For Metal arrays, this will panic. Metal arrays should be accessed via GPU kernels.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        match &mut self.storage {
            Storage::Cpu(vec) => vec.get_mut(index),
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            Storage::Metal { .. } => {
                panic!("Cannot get mutable element from Metal array. Use GPU kernels to access Metal data.");
            }
        }
    }
}

impl<T: Clone> Array<T> {
    /// Create an array filled with a value on the default device
    pub fn filled(value: T, shape: Vec<usize>) -> Self 
    where
        T: Copy,
    {
        Self::filled_on_device(value, shape, Device::default())
    }

    /// Create an array filled with a value on a specific device
    pub fn filled_on_device(value: T, shape: Vec<usize>, device: Device) -> Self 
    where
        T: Copy,
    {
        let len: usize = shape.iter().product();
        let data = vec![value; len];
        Self::new_on_device(data, shape, device)
    }
}

impl<T: Copy> Array<T> {
    /// Get an element by multi-dimensional indices
    /// Note: For Metal arrays, this will panic. Metal arrays should be accessed via GPU kernels.
    pub fn get_at(&self, indices: &[usize]) -> Option<T> {
        if indices.len() != self.shape.len() {
            return None;
        }

        let mut flat_idx = 0;
        let mut stride = 1;

        // Calculate flat index from multi-dimensional indices
        for (i, &idx) in indices.iter().enumerate().rev() {
            if idx >= self.shape[i] {
                return None;
            }
            flat_idx += idx * stride;
            stride *= self.shape[i];
        }

        match &self.storage {
            Storage::Cpu(vec) => vec.get(flat_idx).copied(),
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            Storage::Metal { .. } => {
                panic!("Cannot get element from Metal array. Use GPU kernels to access Metal data.");
            }
        }
    }
}
