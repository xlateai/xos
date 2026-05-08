#[cfg(not(target_os = "ios"))]
use rustpython_vm::PyObjectRef;
use rustpython_vm::{builtins::PyModule, function::FuncArgs, PyRef, PyResult, VirtualMachine};

/// xos.dialoguer.select(prompt, items, default=0) - Show selection dialog
#[cfg(target_os = "ios")]
fn select(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    Err(vm.new_runtime_error(
        "dialoguer.select() is not available on iOS. Use xos.system.get_system_type() to detect iOS and skip selection.".to_string(),
    ))
}

#[cfg(not(target_os = "ios"))]
fn select(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    // Parse arguments
    let prompt: String = if !args.args.is_empty() {
        args.args[0].clone().try_into_value(vm)?
    } else {
        return Err(vm.new_type_error("select() requires 'prompt' argument".to_string()));
    };

    let items_obj: PyObjectRef = if args.args.len() > 1 {
        args.args[1].clone()
    } else {
        return Err(vm.new_type_error("select() requires 'items' argument".to_string()));
    };

    let default_idx: usize = if args.args.len() > 2 {
        args.args[2].clone().try_into_value(vm)?
    } else if let Some(default_arg) = args.kwargs.get("default") {
        default_arg.clone().try_into_value(vm)?
    } else {
        0
    };

    // Convert items to Vec<String>
    let items_list = items_obj
        .downcast_ref::<rustpython_vm::builtins::PyList>()
        .ok_or_else(|| vm.new_type_error("items must be a list".to_string()))?;

    let items_vec = items_list.borrow_vec();
    let mut items_str = Vec::new();

    for item in items_vec.iter() {
        let s: String = item.str(vm)?.to_string();
        items_str.push(s);
    }

    if items_str.is_empty() {
        return Err(vm.new_value_error("items list cannot be empty".to_string()));
    }

    // Use dialoguer to show selection
    use dialoguer::Select;

    let selection = Select::new()
        .with_prompt(&prompt)
        .items(&items_str)
        .default(default_idx)
        .interact()
        .map_err(|e| vm.new_runtime_error(format!("Failed to get user selection: {}", e)))?;

    Ok(vm.ctx.new_int(selection).into())
}

/// Create the dialoguer module
pub fn make_dialoguer_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.dialoguer", vm.ctx.new_dict(), None);

    // Add select function
    let _ = module.set_attr("select", vm.new_function("select", select), vm);

    module
}
