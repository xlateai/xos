//! `xos.mesh` and `xos.input` — same-machine mesh + terminal line editor (Rust-backed).
//! Python surface lives in `bootstrap.py` (included at compile time).

use crate::apps::mesh::runtime::{MeshSession, Packet};
use crate::apps::mesh::state::{LINE_EDITOR, MESH};
use crate::python_api::runtime::format_python_exception;
use rustpython_vm::builtins::{PyDict, PyList, PyModule, PyTuple};
use rustpython_vm::function::FuncArgs;
use rustpython_vm::{PyRef, PyResult, VirtualMachine};
use rustpython_vm::AsObject;

const MESH_BOOTSTRAP: &str = include_str!("bootstrap.py");

/// Convert a Python value to JSON without importing Python's `json` module (RustPython may omit it).
fn py_to_json(
    vm: &VirtualMachine,
    obj: rustpython_vm::PyObjectRef,
) -> Result<serde_json::Value, rustpython_vm::builtins::PyBaseExceptionRef> {
    py_to_json_inner(vm, obj, 0)
}

fn py_to_json_inner(
    vm: &VirtualMachine,
    obj: rustpython_vm::PyObjectRef,
    depth: u32,
) -> Result<serde_json::Value, rustpython_vm::builtins::PyBaseExceptionRef> {
    if depth > 48 {
        return Err(vm.new_value_error(
            "mesh payload: nesting too deep (max 48)".to_string(),
        ));
    }
    if vm.is_none(&obj) {
        return Ok(serde_json::Value::Null);
    }
    if let Ok(b) = obj.clone().try_into_value::<bool>(vm) {
        return Ok(serde_json::Value::Bool(b));
    }
    if let Ok(i) = obj.clone().try_into_value::<i64>(vm) {
        return Ok(serde_json::Value::Number(i.into()));
    }
    if let Ok(f) = obj.clone().try_into_value::<f64>(vm) {
        return Ok(
            serde_json::Number::from_f64(f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
        );
    }
    if let Ok(s) = obj.clone().try_into_value::<String>(vm) {
        return Ok(serde_json::Value::String(s));
    }
    if let Some(list) = obj.downcast_ref::<PyList>() {
        let mut arr = Vec::with_capacity(list.borrow_vec().len());
        for item in list.borrow_vec().iter() {
            arr.push(py_to_json_inner(vm, item.clone(), depth + 1)?);
        }
        return Ok(serde_json::Value::Array(arr));
    }
    if let Some(tup) = obj.downcast_ref::<PyTuple>() {
        let mut arr = Vec::with_capacity(tup.as_slice().len());
        for item in tup.as_slice().iter() {
            arr.push(py_to_json_inner(vm, item.clone(), depth + 1)?);
        }
        return Ok(serde_json::Value::Array(arr));
    }
    // Direct `PyDict` iteration — no `list(dict.items())` allocation (see rustpython `IntoIterator for &PyDict`).
    if let Some(dict) = obj.downcast_ref::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (key, value) in dict {
            let key_str = key.str(vm)?.to_string();
            map.insert(key_str, py_to_json_inner(vm, value, depth + 1)?);
        }
        return Ok(serde_json::Value::Object(map));
    }

    Err(vm.new_type_error(
        "mesh payload must be JSON-serializable: use None, bool, int, float, str, list, tuple, or dict"
            .to_string(),
    ))
}

fn json_to_py(vm: &VirtualMachine, v: &serde_json::Value) -> rustpython_vm::PyObjectRef {
    match v {
        serde_json::Value::Null => vm.ctx.none(),
        serde_json::Value::Bool(b) => vm.ctx.new_bool(*b).into(),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                vm.ctx.new_int(i).into()
            } else if let Some(f) = n.as_f64() {
                vm.ctx.new_float(f).into()
            } else {
                vm.ctx.new_str(n.to_string()).into()
            }
        }
        serde_json::Value::String(s) => vm.ctx.new_str(s.as_str()).into(),
        serde_json::Value::Array(a) => {
            let items: Vec<rustpython_vm::PyObjectRef> =
                a.iter().map(|x| json_to_py(vm, x)).collect();
            vm.ctx.new_list(items).into()
        }
        serde_json::Value::Object(o) => {
            let d = vm.ctx.new_dict();
            for (k, val) in o {
                let _ = d.set_item(k, json_to_py(vm, val), vm);
            }
            d.into()
        }
    }
}

fn packet_to_py(vm: &VirtualMachine, p: &Packet) -> PyResult {
    let dict = vm.ctx.new_dict();
    dict.set_item(
        "from_rank",
        vm.ctx.new_int(p.from_rank as isize).into(),
        vm,
    )?;
    match &p.body {
        serde_json::Value::Object(o) => {
            for (k, v) in o {
                dict.set_item(k.as_str(), json_to_py(vm, v), vm)?;
            }
        }
        _ => {
            dict.set_item("value", json_to_py(vm, &p.body), vm)?;
        }
    }
    Ok(dict.into())
}

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
fn mesh_connect(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let session: String = if let Some(s) = args.args.first() {
        s.clone().try_into_value(vm)?
    } else {
        "default".to_string()
    };
    let session = MeshSession::join(&session).map_err(|e| vm.new_runtime_error(e))?;
    *MESH.lock().unwrap() = Some(std::sync::Arc::new(session));
    Ok(vm.ctx.none())
}

#[cfg(any(target_arch = "wasm32", target_os = "ios"))]
fn mesh_connect(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    Err(vm.new_runtime_error(
        "xos.mesh is not available on this target".to_string(),
    ))
}

fn mesh_rank(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let g = MESH.lock().unwrap();
    let Some(m) = g.as_ref() else {
        return Err(vm.new_runtime_error(
            "mesh not connected; call xos.mesh.connect()".to_string(),
        ));
    };
    Ok(vm.ctx.new_int(m.rank as isize).into())
}

fn mesh_num_nodes(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let g = MESH.lock().unwrap();
    let Some(m) = g.as_ref() else {
        return Err(vm.new_runtime_error(
            "mesh not connected; call xos.mesh.connect()".to_string(),
        ));
    };
    Ok(vm
        .ctx
        .new_int(m.current_num_nodes() as isize)
        .into())
}

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
fn mesh_broadcast_payload(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let id: String = args
        .args
        .get(0)
        .ok_or_else(|| vm.new_type_error("broadcast requires id".to_string()))?
        .clone()
        .try_into_value(vm)?;
    let payload_obj = args
        .args
        .get(1)
        .ok_or_else(|| vm.new_type_error("broadcast requires payload dict".to_string()))?
        .clone();
    let payload = py_to_json(vm, payload_obj)?;
    let g = MESH.lock().unwrap();
    let Some(m) = g.as_ref() else {
        return Err(vm.new_runtime_error("mesh not connected".to_string()));
    };
    m.broadcast_json(&id, payload)
        .map_err(|e| vm.new_runtime_error(e))?;
    Ok(vm.ctx.none())
}

#[cfg(any(target_arch = "wasm32", target_os = "ios"))]
fn mesh_broadcast_payload(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    Err(vm.new_runtime_error("mesh not available".to_string()))
}

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
fn mesh_send_payload(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let id: String = args
        .args
        .get(0)
        .ok_or_else(|| vm.new_type_error("send requires id".to_string()))?
        .clone()
        .try_into_value(vm)?;
    let to_obj = args
        .args
        .get(1)
        .ok_or_else(|| vm.new_type_error("send requires to".to_string()))?;
    let payload_obj = args
        .args
        .get(2)
        .ok_or_else(|| vm.new_type_error("send requires payload dict".to_string()))?
        .clone();
    let payload = py_to_json(vm, payload_obj)?;
    let g = MESH.lock().unwrap();
    let Some(m) = g.as_ref() else {
        return Err(vm.new_runtime_error("mesh not connected".to_string()));
    };
    if vm.is_none(to_obj) {
        m.broadcast_json(&id, payload)
            .map_err(|e| vm.new_runtime_error(e))?;
    } else {
        let to: i32 = to_obj.clone().try_into_value(vm)?;
        m.send_to_json(to as u32, &id, payload)
            .map_err(|e| vm.new_runtime_error(e))?;
    }
    Ok(vm.ctx.none())
}

#[cfg(any(target_arch = "wasm32", target_os = "ios"))]
fn mesh_send_payload(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    Err(vm.new_runtime_error("mesh not available".to_string()))
}

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
fn mesh_receive(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let id: String = args
        .args
        .get(0)
        .ok_or_else(|| vm.new_type_error("receive requires id".to_string()))?
        .clone()
        .try_into_value(vm)?;
    let wait = args
        .args
        .get(1)
        .map(|o| o.clone().try_into_value::<bool>(vm))
        .transpose()?
        .unwrap_or(true);
    let latest_only = args
        .args
        .get(2)
        .map(|o| o.clone().try_into_value::<bool>(vm))
        .transpose()?
        .unwrap_or(false);

    let g = MESH.lock().unwrap();
    let Some(m) = g.as_ref() else {
        return Err(vm.new_runtime_error("mesh not connected".to_string()));
    };
    let inbox = m.inbox();
    drop(g);

    let packs = inbox
        .receive(&id, wait, latest_only)
        .map_err(|e| vm.new_runtime_error(e))?;

    let Some(packs) = packs else {
        if !latest_only {
            return Ok(vm.ctx.new_list(vec![]).into());
        }
        return Ok(vm.ctx.none());
    };

    if latest_only {
        let one = packs.into_iter().next().unwrap();
        return packet_to_py(vm, &one);
    }

    let mut out = Vec::new();
    for p in packs {
        out.push(packet_to_py(vm, &p)?);
    }
    Ok(vm.ctx.new_list(out).into())
}

#[cfg(any(target_arch = "wasm32", target_os = "ios"))]
fn mesh_receive(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    Err(vm.new_runtime_error("mesh not available".to_string()))
}

fn xos_input(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let prompt: String = if let Some(o) = args.args.get(0) {
        o.clone().try_into_value(vm)?
    } else if let Some(o) = args.kwargs.get("prompt") {
        o.clone().try_into_value(vm)?
    } else {
        ">>> ".to_string()
    };

    // `xos.input(">>> ", wait=False)` passes `wait` as a **keyword** — it is not `args[1]`.
    let wait: bool = if let Some(w) = args.args.get(1) {
        w.clone().try_into_value(vm)?
    } else if let Some(w) = args.kwargs.get("wait") {
        w.clone().try_into_value(vm)?
    } else {
        true
    };

    let ed = LINE_EDITOR.lock().unwrap();
    let Some(editor) = ed.as_ref() else {
        if wait {
            use std::io::{self, BufRead, Write};
            let mut s = String::new();
            print!("{}", prompt);
            let _ = io::stdout().flush();
            let n = io::stdin().lock().read_line(&mut s).unwrap_or(0);
            if n == 0 {
                return Ok(vm.ctx.none());
            }
            return Ok(vm.ctx.new_str(s.trim_end_matches('\n')).into());
        }
        return Ok(vm.ctx.none());
    };

    let mut inner = editor.lock().unwrap();
    inner.set_prompt(prompt);
    match inner.read_line(wait) {
        Ok(Some(s)) => Ok(vm.ctx.new_str(s.as_str()).into()),
        Ok(None) => Ok(vm.ctx.none()),
        Err(e) => Err(vm.new_runtime_error(e)),
    }
}

pub fn register_mesh(module: &PyRef<PyModule>, vm: &VirtualMachine) {
    let sub = vm.new_module("xos.mesh", vm.ctx.new_dict(), None);

    let _ = sub.set_attr(
        "_mesh_connect",
        vm.new_function("_mesh_connect", mesh_connect),
        vm,
    );
    let _ = sub.set_attr("_mesh_rank", vm.new_function("_mesh_rank", mesh_rank), vm);
    let _ = sub.set_attr(
        "_mesh_num_nodes",
        vm.new_function("_mesh_num_nodes", mesh_num_nodes),
        vm,
    );
    let _ = sub.set_attr(
        "_mesh_broadcast_payload",
        vm.new_function("_mesh_broadcast_payload", mesh_broadcast_payload),
        vm,
    );
    let _ = sub.set_attr(
        "_mesh_send_payload",
        vm.new_function("_mesh_send_payload", mesh_send_payload),
        vm,
    );
    let _ = sub.set_attr(
        "_mesh_receive",
        vm.new_function("_mesh_receive", mesh_receive),
        vm,
    );

    let scope = vm.new_scope_with_builtins();
    let _ = scope.globals.set_item(
        "_mesh_connect",
        sub.get_attr("_mesh_connect", vm).unwrap(),
        vm,
    );
    let _ = scope.globals.set_item("_mesh_rank", sub.get_attr("_mesh_rank", vm).unwrap(), vm);
    let _ = scope
        .globals
        .set_item("_mesh_num_nodes", sub.get_attr("_mesh_num_nodes", vm).unwrap(), vm);
    let _ = scope.globals.set_item(
        "_mesh_broadcast_payload",
        sub.get_attr("_mesh_broadcast_payload", vm).unwrap(),
        vm,
    );
    let _ = scope.globals.set_item(
        "_mesh_send_payload",
        sub.get_attr("_mesh_send_payload", vm).unwrap(),
        vm,
    );
    let _ = scope.globals.set_item(
        "_mesh_receive",
        sub.get_attr("_mesh_receive", vm).unwrap(),
        vm,
    );

    match vm.run_code_string(scope.clone(), MESH_BOOTSTRAP, "<xos.mesh/bootstrap.py>".to_string()) {
        Ok(_) => {
            if let Ok(connect_fn) = scope.globals.get_item("connect", vm) {
                let _ = sub.set_attr("connect", connect_fn, vm);
            }
        }
        Err(py_exc) => {
            eprintln!(
                "xos.mesh bootstrap failed:\n{}",
                format_python_exception(vm, &py_exc)
            );
        }
    }

    let _ = module.set_attr("mesh", sub.as_object().to_owned(), vm);
    let _ = module.set_attr("input", vm.new_function("input", xos_input), vm);
}
