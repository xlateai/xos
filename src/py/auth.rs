use rustpython_vm::{PyRef, PyResult, VirtualMachine, builtins::PyModule, function::FuncArgs};

#[cfg(not(target_arch = "wasm32"))]
fn auth_username(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let username = crate::auth::load_identity()
        .map(|id| id.username)
        .unwrap_or_default();
    Ok(vm.ctx.new_str(username).into())
}

#[cfg(target_arch = "wasm32")]
fn auth_username(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    Ok(vm.ctx.new_str("").into())
}

#[cfg(not(target_arch = "wasm32"))]
fn auth_node_name(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let node_name = crate::auth::load_node_identity()
        .map(|id| id.node_name)
        .unwrap_or_default();
    Ok(vm.ctx.new_str(node_name).into())
}

#[cfg(target_arch = "wasm32")]
fn auth_node_name(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    Ok(vm.ctx.new_str("").into())
}

#[cfg(not(target_arch = "wasm32"))]
fn auth_node_uuid(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    // Stable per-node id derived from node public key (hex string).
    let node_uuid = crate::auth::load_node_identity()
        .map(|id| id.node_id())
        .unwrap_or_default();
    Ok(vm.ctx.new_str(node_uuid).into())
}

#[cfg(target_arch = "wasm32")]
fn auth_node_uuid(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    Ok(vm.ctx.new_str("").into())
}

pub fn make_auth_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.auth", vm.ctx.new_dict(), None);
    let _ = module.set_attr("username", vm.new_function("username", auth_username), vm);
    let _ = module.set_attr("node_name", vm.new_function("node_name", auth_node_name), vm);
    let _ = module.set_attr("node_uuid", vm.new_function("node_uuid", auth_node_uuid), vm);
    module
}
