use rustpython_vm::{PyRef, PyResult, VirtualMachine, builtins::PyModule, function::FuncArgs};
#[cfg(not(target_arch = "wasm32"))]
use std::process::{Command, Stdio};

fn manager_num_procs(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    Ok(vm.ctx.new_int(crate::manager::num_processes() as isize).into())
}

fn manager_version(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    Ok(vm.ctx.new_int(crate::manager::snapshot_version() as isize).into())
}

fn manager_list_procs(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let snaps = crate::manager::list_processes();
    let mut items = Vec::with_capacity(snaps.len());
    for p in snaps {
        let d = vm.ctx.new_dict();
        d.set_item("pid", vm.ctx.new_int(p.pid as isize).into(), vm)?;
        d.set_item("label", vm.ctx.new_str(p.label.as_str()).into(), vm)?;
        d.set_item("rank", vm.ctx.new_int(p.rank as isize).into(), vm)?;
        d.set_item("node_id", vm.ctx.new_str(p.node_id.as_str()).into(), vm)?;
        d.set_item("last_seen_ms", vm.ctx.new_int(p.last_seen_ms as isize).into(), vm)?;
        let mut channels = Vec::with_capacity(p.channels.len());
        for ch in p.channels {
            let c = vm.ctx.new_dict();
            c.set_item("id", vm.ctx.new_str(ch.id.as_str()).into(), vm)?;
            c.set_item("mode", vm.ctx.new_str(ch.mode.as_str()).into(), vm)?;
            channels.push(c.into());
        }
        d.set_item("channels", vm.ctx.new_list(channels).into(), vm)?;
        items.push(d.into());
    }
    Ok(vm.ctx.new_list(items).into())
}

#[cfg(not(target_arch = "wasm32"))]
fn manager_run_xos(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let input: String = args
        .args
        .first()
        .ok_or_else(|| vm.new_type_error("run_xos requires a command string".to_string()))?
        .clone()
        .try_into_value(vm)?;
    let tokens: Vec<String> = input
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();
    if tokens.is_empty() {
        return Err(vm.new_value_error("run_xos: empty command".to_string()));
    }
    if matches!(tokens[0].as_str(), "terminal" | "term") {
        return Err(vm.new_runtime_error(
            "run_xos: refusing recursive terminal launch (`xos terminal`)".to_string(),
        ));
    }

    let exe = std::env::current_exe()
        .map_err(|e| vm.new_runtime_error(format!("run_xos: current_exe failed: {e}")))?;
    let cmd_text = format!("{} {}", exe.display(), tokens.join(" "));

    let detach = matches!(
        tokens[0].as_str(),
        "rs" | "rust" | "app" | "code" | "py" | "python"
    );

    let out = vm.ctx.new_dict();
    out.set_item("cmd", vm.ctx.new_str(cmd_text.as_str()).into(), vm)?;
    out.set_item("detached", vm.ctx.new_bool(detach).into(), vm)?;

    if detach {
        let mut cmd = Command::new(&exe);
        cmd.args(tokens.iter())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = cmd
            .spawn()
            .map_err(|e| vm.new_runtime_error(format!("run_xos spawn failed: {e}")))?;
        out.set_item("ok", vm.ctx.new_bool(true).into(), vm)?;
        out.set_item("pid", vm.ctx.new_int(child.id() as isize).into(), vm)?;
        out.set_item("code", vm.ctx.none(), vm)?;
        out.set_item("stdout", vm.ctx.new_str("").into(), vm)?;
        out.set_item("stderr", vm.ctx.new_str("").into(), vm)?;
        return Ok(out.into());
    }

    let output = Command::new(&exe)
        .args(tokens.iter())
        .output()
        .map_err(|e| vm.new_runtime_error(format!("run_xos exec failed: {e}")))?;
    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    out.set_item("ok", vm.ctx.new_bool(output.status.success()).into(), vm)?;
    out.set_item("pid", vm.ctx.none(), vm)?;
    out.set_item("code", vm.ctx.new_int(code as isize).into(), vm)?;
    out.set_item("stdout", vm.ctx.new_str(stdout.as_str()).into(), vm)?;
    out.set_item("stderr", vm.ctx.new_str(stderr.as_str()).into(), vm)?;
    Ok(out.into())
}

#[cfg(target_arch = "wasm32")]
fn manager_run_xos(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    Err(vm.new_runtime_error("run_xos is not available on wasm".to_string()))
}

pub fn make_manager_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.manager", vm.ctx.new_dict(), None);
    let _ = module.set_attr("num_procs", vm.new_function("num_procs", manager_num_procs), vm);
    let _ = module.set_attr("version", vm.new_function("version", manager_version), vm);
    let _ = module.set_attr("list_procs", vm.new_function("list_procs", manager_list_procs), vm);
    let _ = module.set_attr("run_xos", vm.new_function("run_xos", manager_run_xos), vm);
    module
}
