use rustpython_vm::{PyRef, PyResult, VirtualMachine, builtins::PyModule, function::FuncArgs};

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

pub fn make_manager_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.manager", vm.ctx.new_dict(), None);
    let _ = module.set_attr("num_procs", vm.new_function("num_procs", manager_num_procs), vm);
    let _ = module.set_attr("version", vm.new_function("version", manager_version), vm);
    let _ = module.set_attr("list_procs", vm.new_function("list_procs", manager_list_procs), vm);
    module
}
