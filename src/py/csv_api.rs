//! `xos.csv` — load UTF-8 CSV files with a header row; rows are dicts (`header → cell`).

use rustpython_vm::{
    builtins::PyModule,
    PyRef, PyResult, VirtualMachine, function::FuncArgs,
};
use std::path::PathBuf;

fn parse_csv_file(path: &str) -> Result<(Vec<String>, Vec<Vec<String>>), String> {
    let buf =
        std::fs::read(PathBuf::from(path)).map_err(|e| format!("cannot read {:?}: {}", path, e))?;
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(buf.as_slice());
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| e.to_string())?
        .iter()
        .map(|h| h.trim().to_string())
        .collect();
    if headers.is_empty() {
        return Err("csv: empty header row".into());
    }
    let mut rows: Vec<Vec<String>> = Vec::new();
    for rec in rdr.records() {
        let rec = rec.map_err(|e| e.to_string())?;
        let mut row: Vec<String> = rec.iter().map(|f| f.to_string()).collect();
        while row.len() < headers.len() {
            row.push(String::new());
        }
        row.truncate(headers.len());
        rows.push(row);
    }
    Ok((headers, rows))
}

#[cfg(not(target_arch = "wasm32"))]
fn load_native(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let path_str: String = args.bind(vm)?;
    let (headers, rows) = parse_csv_file(&path_str).map_err(|e| vm.new_os_error(e))?;
    let hlist: Vec<_> = headers
        .iter()
        .map(|s| vm.ctx.new_str(s.as_str()).into())
        .collect();
    let py_headers = vm.ctx.new_list(hlist);
    let mut py_rows = Vec::with_capacity(rows.len());
    for row in rows {
        let cells: Vec<_> = row
            .iter()
            .map(|s| vm.ctx.new_str(s.as_str()).into())
            .collect();
        py_rows.push(vm.ctx.new_list(cells).into());
    }
    let py_row_list = vm.ctx.new_list(py_rows);
    Ok(vm
        .ctx
        .new_tuple(vec![py_headers.into(), py_row_list.into()])
        .into())
}

#[cfg(target_arch = "wasm32")]
fn load_native(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    Err(vm.new_runtime_error(
        "xos.csv._load_native: filesystem CSV load is not available on wasm".into(),
    ))
}

pub fn make_csv_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.csv", vm.ctx.new_dict(), None);
    module
        .set_attr("_load_native", vm.new_function("_load_native", load_native), vm)
        .unwrap();

    let scope = vm.new_scope_with_builtins();
    let load_fn = module.get_attr("_load_native", vm).unwrap();
    scope
        .globals
        .set_item("_load_native", load_fn, vm)
        .unwrap();

    let py_code = r#"
class CsvTable:
    __slots__ = ("_headers", "_rows")

    def __init__(self, headers, rows):
        self._headers = tuple(str(h) for h in headers)
        self._rows = tuple(tuple(str(c) for c in r) for r in rows)

    def __len__(self):
        return len(self._rows)

    def __getitem__(self, index):
        if type(index) is not int:
            raise TypeError("row index must be int")
        n = len(self._rows)
        if index < -n or index >= n:
            raise IndexError("csv row index out of range")
        row = self._rows[index]
        return dict(zip(self._headers, row))

def load(path):
    """Load a UTF-8 CSV (header row). ``table[i]`` returns a dict mapping column name → cell string."""
    p = path.__fspath__() if hasattr(path, "__fspath__") else path
    h, rs = _load_native(str(p))
    headers = list(h)
    rows = [tuple(list(r)) for r in rs]
    return CsvTable(headers, rows)
"#;

    let _ = vm.run_code_string(scope.clone(), py_code, "<xos.csv>".to_string());
    if let Ok(v) = scope.globals.get_item("CsvTable", vm) {
        let _ = module.set_attr("CsvTable", v, vm);
    }
    if let Ok(v) = scope.globals.get_item("load", vm) {
        let _ = module.set_attr("load", v, vm);
    }

    module
}
