/// A typed array of numbers with a shape
#[derive(Debug, Clone)]
pub struct Array<T> {
    /// The data stored in row-major order
    data: Vec<T>,
    /// The shape of the array (dimensions)
    shape: Vec<usize>,
}

impl<T> Array<T> {
    /// Create a new array from data and shape
    pub fn new(data: Vec<T>, shape: Vec<usize>) -> Self {
        let expected_len: usize = shape.iter().product();
        assert_eq!(
            data.len(),
            expected_len,
            "Data length {} does not match shape {:?} (expected {})",
            data.len(),
            shape,
            expected_len
        );
        Self { data, shape }
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
    /// Create an array filled with a value
    pub fn filled(value: T, shape: Vec<usize>) -> Self {
        let len: usize = shape.iter().product();
        Self {
            data: vec![value; len],
            shape,
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
