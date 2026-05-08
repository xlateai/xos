use rustpython_vm::{builtins::PyModule, function::FuncArgs, PyRef, PyResult, VirtualMachine};

#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::process::{Command, Stdio};

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

#[cfg(not(target_arch = "wasm32"))]
fn auth_is_logged_in(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    Ok(vm.ctx.new_bool(crate::auth::is_logged_in()).into())
}

#[cfg(target_arch = "wasm32")]
fn auth_is_logged_in(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    Ok(vm.ctx.new_bool(false).into())
}

#[cfg(not(target_arch = "wasm32"))]
fn daemon_pid_path() -> Option<std::path::PathBuf> {
    crate::auth::auth_data_dir()
        .ok()
        .map(|d| d.join("daemon.pid"))
}

#[cfg(not(target_arch = "wasm32"))]
fn read_daemon_pid() -> Option<u32> {
    let path = daemon_pid_path()?;
    if !path.exists() {
        return None;
    }
    let raw = fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&raw);
    let token = text
        .split(|c: char| !c.is_ascii_digit())
        .find(|part| !part.is_empty())?;
    token.parse::<u32>().ok()
}

#[cfg(all(not(target_arch = "wasm32"), windows))]
fn process_is_running(pid: u32) -> bool {
    let output = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    let Ok(out) = output else {
        return false;
    };
    let txt = String::from_utf8_lossy(&out.stdout);
    !txt.contains("No tasks are running")
}

#[cfg(all(not(target_arch = "wasm32"), not(windows)))]
fn process_is_running(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(not(target_arch = "wasm32"))]
fn auth_daemon_pid(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let pid = read_daemon_pid().unwrap_or(0) as isize;
    Ok(vm.ctx.new_int(pid).into())
}

#[cfg(target_arch = "wasm32")]
fn auth_daemon_pid(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    Ok(vm.ctx.new_int(0).into())
}

#[cfg(not(target_arch = "wasm32"))]
fn auth_daemon_online(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let online = read_daemon_pid().map(process_is_running).unwrap_or(false);
    Ok(vm.ctx.new_bool(online).into())
}

#[cfg(target_arch = "wasm32")]
fn auth_daemon_online(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    Ok(vm.ctx.new_bool(false).into())
}

pub fn make_auth_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.auth", vm.ctx.new_dict(), None);
    let _ = module.set_attr("username", vm.new_function("username", auth_username), vm);
    let _ = module.set_attr(
        "node_name",
        vm.new_function("node_name", auth_node_name),
        vm,
    );
    let _ = module.set_attr(
        "node_uuid",
        vm.new_function("node_uuid", auth_node_uuid),
        vm,
    );
    let _ = module.set_attr(
        "is_logged_in",
        vm.new_function("is_logged_in", auth_is_logged_in),
        vm,
    );
    let _ = module.set_attr(
        "daemon_pid",
        vm.new_function("daemon_pid", auth_daemon_pid),
        vm,
    );
    let _ = module.set_attr(
        "daemon_online",
        vm.new_function("daemon_online", auth_daemon_online),
        vm,
    );
    module
}
