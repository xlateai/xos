use rustpython_vm::{PyResult, VirtualMachine, function::FuncArgs};

/// xos.ops.shift(array, dimension=0, amount=1, fill_value=0.0)
/// Fast shift operation for arrays
pub fn shift(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let (array, dimension, amount, fill_value): (
        rustpython_vm::PyObjectRef,
        Option<i64>,
        Option<i64>,
        Option<f64>
    ) = args.bind(vm)?;
    
    let dimension = dimension.unwrap_or(0);
    let amount = amount.unwrap_or(1);
    let fill_value = fill_value.unwrap_or(0.0);
    
    // Get the _data array from the dict
    let data_obj = array.get_attr("_data", vm)?;
    let data_list = data_obj.downcast::<rustpython_vm::builtins::PyList>()
        .map_err(|_| vm.new_type_error("Array _data must be a list".to_string()))?;
    
    // Get shape
    let shape_obj = array.get_attr("shape", vm)?;
    let shape_tuple = shape_obj.downcast::<rustpython_vm::builtins::PyTuple>()
        .map_err(|_| vm.new_type_error("Array shape must be a tuple".to_string()))?;
    
    let shape: Vec<i64> = {
        let mut result = Vec::new();
        for i in 0..shape_tuple.len() {
            let item = &shape_tuple[i];
            let int_obj = item.downcast_ref::<rustpython_vm::builtins::PyInt>()
                .ok_or_else(|| vm.new_type_error("Shape must contain integers".to_string()))?;
            let val: i64 = int_obj.try_to_primitive(vm)?;
            result.push(val);
        }
        result
    };
    
    if shape.len() != 2 {
        return Err(vm.new_value_error("shift only supports 2D arrays for now".to_string()));
    }
    
    let rows = shape[0] as usize;
    let cols = shape[1] as usize;
    
    if dimension != 0 {
        return Err(vm.new_value_error("shift only supports dimension=0 for now".to_string()));
    }
    
    if amount <= 0 {
        return Err(vm.new_value_error("amount must be positive".to_string()));
    }
    
    let amount = amount as usize;
    
    // Get mutable reference to the list data
    let mut data_vec = data_list.borrow_vec_mut();
    
    // Shift rows: move row i+amount to row i
    // This is shifting "left" along dimension 0 (removing first rows, filling at end)
    let total_len = data_vec.len();
    
    if amount >= rows {
        // Shift entire array - fill with fill_value
        for item in data_vec.iter_mut() {
            *item = vm.ctx.new_float(fill_value).into();
        }
    } else {
        // Shift by moving data
        // Copy from [amount*cols..] to [0..]
        let src_start = amount * cols;
        
        // Use a temporary buffer to avoid ownership issues
        let mut temp: Vec<rustpython_vm::PyObjectRef> = Vec::with_capacity(total_len);
        
        // Copy data that should shift up
        for i in src_start..total_len {
            temp.push(data_vec[i].clone());
        }
        
        // Fill the end with fill_value
        for _ in 0..(amount * cols) {
            temp.push(vm.ctx.new_float(fill_value).into());
        }
        
        // Write back
        for (i, item) in temp.into_iter().enumerate() {
            data_vec[i] = item;
        }
    }
    
    Ok(vm.ctx.none())
}

