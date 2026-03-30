use rustpython_vm::{PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs};
use include_dir::{Dir, include_dir};

// Include the example-scripts directory at compile time
static PYTHON_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/example-scripts");

/// xos.data.list() - List all files and directories
fn list(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let mut entries = Vec::new();
    
    // Collect all files from the embedded directory
    fn collect_entries(dir: &Dir, base_path: &str, entries: &mut Vec<String>) {
        // Add all files in this directory
        for file in dir.files() {
            if let Some(filename) = file.path().file_name() {
                let relative_path = if base_path.is_empty() {
                    filename.to_string_lossy().to_string()
                } else {
                    format!("{}/{}", base_path, filename.to_string_lossy())
                };
                entries.push(relative_path);
            }
        }
        
        // Add all subdirectories
        for subdir in dir.dirs() {
            if let Some(dirname) = subdir.path().file_name() {
                let relative_path = if base_path.is_empty() {
                    dirname.to_string_lossy().to_string()
                } else {
                    format!("{}/{}", base_path, dirname.to_string_lossy())
                };
                entries.push(format!("{}/", relative_path)); // Add trailing slash for directories
                
                // Recursively collect from subdirectories
                collect_entries(subdir, &relative_path, entries);
            }
        }
    }
    
    collect_entries(&PYTHON_DIR, "", &mut entries);
    entries.sort();
    
    // Return a Python list of all entries
    let py_entries: Vec<_> = entries
        .iter()
        .map(|entry| vm.ctx.new_str(entry.as_str()).into())
        .collect();
    
    Ok(vm.ctx.new_list(py_entries).into())
}

/// Helper function to find a file in the embedded directory
fn find_file<'a>(path: &str) -> Option<&'a [u8]> {
    PYTHON_DIR.get_file(path).map(|f| f.contents())
}

/// xos.data.read(path) - Read entire file as string
fn read(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let path: String = args.bind(vm)?;
    
    match find_file(&path) {
        Some(contents) => {
            match std::str::from_utf8(contents) {
                Ok(text) => Ok(vm.ctx.new_str(text).into()),
                Err(_) => Err(vm.new_value_error(format!("File '{}' contains invalid UTF-8", path))),
            }
        }
        None => Err(vm.new_os_error(format!("File not found: {}", path))),
    }
}

/// xos.data.read_line(path, line) - Read a specific line from a file (0-indexed)
fn read_line(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let (path, line_num): (String, usize) = args.bind(vm)?;
    
    match find_file(&path) {
        Some(contents) => {
            match std::str::from_utf8(contents) {
                Ok(text) => {
                    let lines: Vec<&str> = text.lines().collect();
                    if line_num < lines.len() {
                        Ok(vm.ctx.new_str(lines[line_num]).into())
                    } else {
                        Err(vm.new_index_error(format!("Line {} out of range (file has {} lines)", line_num, lines.len())))
                    }
                }
                Err(_) => Err(vm.new_value_error(format!("File '{}' contains invalid UTF-8", path))),
            }
        }
        None => Err(vm.new_os_error(format!("File not found: {}", path))),
    }
}

/// xos.data.read_lines(path, start, end) - Read lines from a file with optional range
fn read_lines(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    // Manually parse arguments to support optional parameters
    let path: String = args.args.get(0)
        .ok_or_else(|| vm.new_type_error("read_lines() missing required argument: 'path'".to_string()))?
        .clone()
        .try_into_value(vm)?;
    
    let start: Option<usize> = if args.args.len() > 1 {
        let start_arg = &args.args[1];
        if vm.is_none(start_arg) {
            None
        } else {
            Some(start_arg.clone().try_into_value(vm)?)
        }
    } else {
        None
    };
    
    let end: Option<usize> = if args.args.len() > 2 {
        let end_arg = &args.args[2];
        if vm.is_none(end_arg) {
            None
        } else {
            Some(end_arg.clone().try_into_value(vm)?)
        }
    } else {
        None
    };
    
    match find_file(&path) {
        Some(contents) => {
            match std::str::from_utf8(contents) {
                Ok(text) => {
                    let lines: Vec<&str> = text.lines().collect();
                    let start_idx = start.unwrap_or(0);
                    let end_idx = end.unwrap_or(lines.len()).min(lines.len());
                    
                    if start_idx > lines.len() {
                        return Err(vm.new_index_error(format!("Start index {} out of range (file has {} lines)", start_idx, lines.len())));
                    }
                    
                    // Create a Python list with the requested lines
                    let py_lines: Vec<_> = lines[start_idx..end_idx]
                        .iter()
                        .map(|line| vm.ctx.new_str(*line).into())
                        .collect();
                    
                    Ok(vm.ctx.new_list(py_lines).into())
                }
                Err(_) => Err(vm.new_value_error(format!("File '{}' contains invalid UTF-8", path))),
            }
        }
        None => Err(vm.new_os_error(format!("File not found: {}", path))),
    }
}

/// Create the data submodule
pub fn make_data_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.data", vm.ctx.new_dict(), None);
    
    // Add functions
    let _ = module.set_attr("list", vm.new_function("list", list), vm);
    let _ = module.set_attr("read", vm.new_function("read", read), vm);
    let _ = module.set_attr("read_line", vm.new_function("read_line", read_line), vm);
    let _ = module.set_attr("read_lines", vm.new_function("read_lines", read_lines), vm);
    
    module
}

