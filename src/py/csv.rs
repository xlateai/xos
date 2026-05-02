//! `xos.csv` — small in-memory CSV table (header + rows) for vocab-sized files (~3MB).

use once_cell::sync::Lazy;
use rustpython_vm::{
    builtins::{PyDict, PyModule},
    function::FuncArgs,
    PyRef, PyResult, VirtualMachine,
};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

struct CsvTable {
    header: Vec<String>,
    rows: Vec<Vec<String>>,
}

static TABLES: Lazy<Mutex<HashMap<u64, CsvTable>>> = Lazy::new(|| Mutex::new(HashMap::new()));
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn next_handle() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

fn csv_id_from_handle_obj(obj: rustpython_vm::PyObjectRef, vm: &VirtualMachine) -> PyResult<u64> {
    if let Some(dict) = obj.downcast_ref::<PyDict>() {
        let id_o = dict
            .get_item("_xos_csv_id", vm)?
            .ok_or_else(|| vm.new_type_error("expected csv table dict from xos.csv.load()".to_string()))?;
        let id: i64 = id_o.clone().try_into_value(vm)?;
        if id <= 0 {
            return Err(vm.new_value_error("invalid csv handle".to_string()));
        }
        return Ok(id as u64);
    }
    Err(vm.new_type_error("expected dict from xos.csv.load()".to_string()))
}

/// xos.csv.load(path) — read full UTF-8 CSV (with header row) into memory.
fn csv_load(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let path: String = args.bind(vm)?;
    let path = Path::new(&path.trim());
    let mut rd = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_path(path)
        .map_err(|e| vm.new_os_error(format!("csv open {}: {}", path.display(), e)))?;
    let header: Vec<String> = rd
        .headers()
        .map_err(|e| vm.new_os_error(format!("csv headers: {e}")))?
        .iter()
        .map(|s| s.to_string())
        .collect();

    let mut rows: Vec<Vec<String>> = Vec::new();
    for rec in rd.records() {
        let rec =
            rec.map_err(|e| vm.new_os_error(format!("csv record: {e}")))?;
        rows.push(rec.iter().map(|s| s.to_string()).collect());
    }

    let id = next_handle();
    let tbl = CsvTable { header, rows };
    TABLES
        .lock()
        .map_err(|_| vm.new_runtime_error("csv table lock poisoned".to_string()))?
        .insert(id, tbl);

    let d = vm.ctx.new_dict();
    d.set_item("_xos_csv_id", vm.ctx.new_int(id as usize).into(), vm)?;
    Ok(d.into())
}

/// xos.csv.len(table) → number of data rows (excluding header).
fn csv_len(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    let handle_obj = args_vec
        .get(0)
        .ok_or_else(|| vm.new_type_error("csv.len(table)".to_string()))?;
    let id = csv_id_from_handle_obj(handle_obj.clone(), vm)?;
    let guard = TABLES
        .lock()
        .map_err(|_| vm.new_runtime_error("csv table lock poisoned".to_string()))?;
    let t = guard
        .get(&id)
        .ok_or_else(|| vm.new_value_error("invalid or closed csv handle".to_string()))?;
    Ok(vm.ctx.new_int(t.rows.len()).into())
}

fn row_dict(vm: &VirtualMachine, t: &CsvTable, row_idx: usize) -> PyResult {
    let row = t
        .rows
        .get(row_idx)
        .ok_or_else(|| vm.new_index_error("csv row index out of range".to_string()))?;
    if row.len() != t.header.len() {
        return Err(vm.new_value_error(format!(
            "csv row {}: column count {} != header {}",
            row_idx,
            row.len(),
            t.header.len()
        )));
    }
    let d = vm.ctx.new_dict();
    for (k, v) in t.header.iter().zip(row.iter()) {
        d.set_item(k.as_str(), vm.ctx.new_str(v.as_str()).into(), vm)?;
    }
    Ok(d.into())
}

/// xos.csv.row(table, index) → dict keyed by header names.
fn csv_row(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let handle_obj = args
        .args
        .get(0)
        .ok_or_else(|| vm.new_type_error("csv.row(table, row_index)".to_string()))?;
    let row_idx: isize = args
        .args
        .get(1)
        .ok_or_else(|| vm.new_type_error("csv.row(table, row_index)".to_string()))?
        .clone()
        .try_into_value(vm)?;
    if row_idx < 0 {
        return Err(vm.new_index_error("row index must be >= 0".to_string()));
    }
    let row_idx = row_idx as usize;
    let id = csv_id_from_handle_obj(handle_obj.clone(), vm)?;
    let guard = TABLES
        .lock()
        .map_err(|_| vm.new_runtime_error("csv table lock poisoned".to_string()))?;
    let t = guard
        .get(&id)
        .ok_or_else(|| vm.new_value_error("invalid or closed csv handle".to_string()))?;
    row_dict(vm, t, row_idx)
}

/// xos.csv.close(table) — drop buffered rows (optional; process exit also frees).
fn csv_close(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let handle_obj = args
        .args
        .get(0)
        .ok_or_else(|| vm.new_type_error("csv.close(table)".to_string()))?;
    let id = csv_id_from_handle_obj(handle_obj.clone(), vm)?;
    let mut guard = TABLES
        .lock()
        .map_err(|_| vm.new_runtime_error("csv table lock poisoned".to_string()))?;
    let _ = guard.remove(&id);
    Ok(vm.ctx.none())
}

pub fn make_csv_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let m = vm.new_module("xos.csv", vm.ctx.new_dict(), None);
    let _ = m.set_attr("load", vm.new_function("load", csv_load), vm);
    let _ = m.set_attr("len", vm.new_function("len", csv_len), vm);
    let _ = m.set_attr("row", vm.new_function("row", csv_row), vm);
    let _ = m.set_attr("close", vm.new_function("close", csv_close), vm);
    m
}
