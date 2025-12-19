use pyo3::prelude::*;
use pyo3::types::PyList;
use crate::tensor::array::{Array, Device};

/// Python bindings for `xos::tensor::array::Array`
#[pymodule]
pub fn array(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyArray>()?;
    m.add_function(wrap_pyfunction!(empty, m)?)?;
    Ok(())
}

#[pyfunction]
fn empty(shape: Vec<usize>) -> PyResult<PyArray> {
    let array = Array::filled(0.0f32, shape);
    Ok(PyArray { inner: array })
}

#[pyclass(name = "Array")]
pub struct PyArray {
    inner: Array<f32>,
}

#[pymethods]
impl PyArray {
    #[new]
    fn new(data: Vec<f32>, shape: Vec<usize>) -> PyResult<Self> {
        let array = Array::new(data, shape);
        Ok(PyArray { inner: array })
    }

    /// Create an empty array filled with zeros
    #[staticmethod]
    fn empty(shape: Vec<usize>) -> PyResult<Self> {
        let array = Array::filled(0.0f32, shape);
        Ok(PyArray { inner: array })
    }

    /// Get the shape of the array
    fn shape(&self) -> Vec<usize> {
        self.inner.shape().to_vec()
    }

    /// Get the number of dimensions
    fn ndim(&self) -> usize {
        self.inner.ndim()
    }

    /// Get the total number of elements
    fn len(&self) -> usize {
        self.inner.len()
    }

    /// Check if the array is empty
    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Get the data as a Python list
    fn tolist(&self) -> PyResult<PyObject> {
        Python::with_gil(|py| {
            let data = self.inner.data();
            let list = PyList::new(py, data);
            Ok(list.into())
        })
    }

    /// NumPy-style string representation
    fn __repr__(&self) -> String {
        format_array_numpy_style(&self.inner)
    }

    /// NumPy-style string representation (same as __repr__)
    fn __str__(&self) -> String {
        self.__repr__()
    }
}

/// Format array in NumPy style
fn format_array_numpy_style(array: &Array<f32>) -> String {
    let shape = array.shape();
    let data = array.data();
    
    if data.is_empty() {
        return format!("array([], shape={:?})", shape);
    }
    
    // For 1D arrays, show simple list
    if shape.len() == 1 {
        let values: Vec<String> = data.iter()
            .map(|v| format_float(*v))
            .collect();
        return format!("array([{}], shape={:?})", values.join(", "), shape);
    }
    
    // For multi-dimensional arrays, show nested structure
    format_multi_dim_array(data, shape)
}

fn format_float(v: f32) -> String {
    if v == 0.0 {
        "0."
    } else if v.fract() == 0.0 {
        format!("{:.1}", v)
    } else {
        format!("{:.6}", v).trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn format_multi_dim_array(data: &[f32], shape: &[usize]) -> String {
    // Calculate strides for indexing
    let mut strides = vec![1; shape.len()];
    for i in (0..shape.len() - 1).rev() {
        strides[i] = strides[i + 1] * shape[i + 1];
    }
    
    // Format recursively
    let formatted = format_array_recursive(data, shape, &strides, 0, 0);
    format!("array({}, shape={:?})", formatted, shape)
}

fn format_array_recursive(
    data: &[f32],
    shape: &[usize],
    strides: &[usize],
    offset: usize,
    depth: usize,
) -> String {
    if shape.is_empty() {
        return String::new();
    }
    
    if shape.len() == 1 {
        // Base case: 1D array
        let mut result = String::from("[");
        for i in 0..shape[0] {
            if i > 0 {
                result.push_str(", ");
            }
            let idx = offset + i * strides[0];
            result.push_str(&format_float(data[idx]));
        }
        result.push(']');
        return result;
    }
    
    // Recursive case: multi-dimensional
    let mut result = String::from("[");
    let sub_shape = &shape[1..];
    let sub_strides = &strides[1..];
    
    for i in 0..shape[0] {
        if i > 0 {
            result.push_str(",\n");
            // Add indentation
            for _ in 0..depth + 1 {
                result.push_str(" ");
            }
        }
        let new_offset = offset + i * strides[0];
        let sub_result = format_array_recursive(data, sub_shape, sub_strides, new_offset, depth + 1);
        result.push_str(&sub_result);
    }
    result.push(']');
    result
}

