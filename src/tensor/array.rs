/// Device where array data resides
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    /// CPU memory
    Cpu,
    /// Metal GPU (macOS/iOS only)
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    Metal,
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
#[derive(Debug, Clone)]
pub struct Array<T> {
    /// The data stored in row-major order
    data: Vec<T>,
    /// The shape of the array (dimensions)
    shape: Vec<usize>,
    /// The device where this array resides
    device: Device,
}

impl<T> Array<T> {
    /// Create a new array from data and shape on the default device
    pub fn new(data: Vec<T>, shape: Vec<usize>) -> Self {
        Self::new_on_device(data, shape, Device::default())
    }

    /// Create a new array from data and shape on a specific device
    pub fn new_on_device(data: Vec<T>, shape: Vec<usize>, device: Device) -> Self {
        let expected_len: usize = shape.iter().product();
        assert_eq!(
            data.len(),
            expected_len,
            "Data length {} does not match shape {:?} (expected {})",
            data.len(),
            shape,
            expected_len
        );
        Self { data, shape, device }
    }

    /// Get the device where this array resides
    pub fn device(&self) -> Device {
        self.device
    }

    /// Move array to a different device (currently only supports CPU, Metal conversion not yet implemented)
    pub fn to_device(&self, target_device: Device) -> Self 
    where
        T: Clone,
    {
        if self.device == target_device {
            return self.clone();
        }
        // For now, just clone and change device marker
        // TODO: Implement actual device transfer when GPU memory is added
        let mut result = self.clone();
        result.device = target_device;
        result
    }

    /// Get the shape of the array
    pub fn shape(&self) -> &[usize] {
        &self.shape
    }

    /// Get the data as a slice
    pub fn data(&self) -> &[T] {
        &self.data
    }

    /// Get mutable access to the data
    pub fn data_mut(&mut self) -> &mut [T] {
        &mut self.data
    }

    /// Get the number of dimensions
    pub fn ndim(&self) -> usize {
        self.shape.len()
    }

    /// Get the total number of elements
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if the array is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Get an element by flat index
    pub fn get(&self, index: usize) -> Option<&T> {
        self.data.get(index)
    }

    /// Get a mutable element by flat index
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.data.get_mut(index)
    }
}

impl<T: Clone> Array<T> {
    /// Create an array filled with a value on the default device
    pub fn filled(value: T, shape: Vec<usize>) -> Self {
        Self::filled_on_device(value, shape, Device::default())
    }

    /// Create an array filled with a value on a specific device
    pub fn filled_on_device(value: T, shape: Vec<usize>, device: Device) -> Self {
        let len: usize = shape.iter().product();
        Self {
            data: vec![value; len],
            shape,
            device,
        }
    }
}

impl<T: Copy> Array<T> {
    /// Get an element by multi-dimensional indices
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

        self.data.get(flat_idx).copied()
    }
}
