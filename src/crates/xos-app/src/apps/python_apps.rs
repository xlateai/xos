//! Discover and launch Python windowed apps from `src/apps/<name>/<name>.py`.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rustpython_vm::Interpreter;
use xos_core::engine::Application;
use xos_core::find_xos_project_root;
use xos_python::engine::pyapp::PyApp;
use xos_python::runtime::execute_python_code;

/// One app folder under `src/apps/`.
#[derive(Debug, Clone)]
pub struct PythonAppDescriptor {
    pub name: String,
    pub app_dir: PathBuf,
    pub main_py: PathBuf,
}

#[derive(Debug, Default)]
pub struct DiscoverResult {
    pub apps: Vec<PythonAppDescriptor>,
    pub warnings: Vec<String>,
}

/// `src/apps` at the repository root.
pub fn apps_dir(project_root: &Path) -> PathBuf {
    project_root.join("src").join("apps")
}

fn is_valid_app_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && !name.starts_with('.')
}

/// Scan `src/apps/*/<name>.py`. Skips invalid folders with warnings; errors on duplicate names.
pub fn discover_python_apps(
    project_root: &Path,
    reserved_names: &[&str],
) -> Result<DiscoverResult, String> {
    let root = apps_dir(project_root);
    if !root.is_dir() {
        return Ok(DiscoverResult::default());
    }

    let mut result = DiscoverResult::default();
    let mut seen = std::collections::HashSet::new();

    let mut entries: Vec<_> = std::fs::read_dir(&root)
        .map_err(|e| format!("failed to read {}: {e}", root.display()))?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(e) => {
                result
                    .warnings
                    .push(format!("{}: {e}", entry.path().display()));
                continue;
            }
        };
        if !file_type.is_dir() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().into_owned();
        if !is_valid_app_name(&name) {
            result.warnings.push(format!(
                "skipped {:?}: app folder name must be alphanumeric/underscore/hyphen",
                name
            ));
            continue;
        }

        let main_py = entry.path().join(format!("{name}.py"));
        if !main_py.is_file() {
            result.warnings.push(format!(
                "skipped {:?}: expected entrypoint {}",
                name,
                main_py.display()
            ));
            continue;
        }

        if reserved_names.iter().any(|r| r.eq_ignore_ascii_case(&name)) {
            return Err(format!(
                "python app {:?} conflicts with an existing native app command",
                name
            ));
        }
        if !seen.insert(name.clone()) {
            return Err(format!("duplicate python app name {:?}", name));
        }

        result.apps.push(PythonAppDescriptor {
            name,
            app_dir: entry.path(),
            main_py,
        });
    }

    result.apps.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
}

pub fn discover_python_apps_from_repo(reserved_names: &[&str]) -> Result<DiscoverResult, String> {
    let root = find_xos_project_root().map_err(|e| e.to_string())?;
    discover_python_apps(&root, reserved_names)
}

pub fn python_app_names(reserved_names: &[&str]) -> Result<Vec<String>, String> {
    Ok(discover_python_apps_from_repo(reserved_names)?
        .apps
        .into_iter()
        .map(|a| a.name)
        .collect())
}

fn escape_python_string_literal(contents: &str) -> String {
    let mut out = String::with_capacity(contents.len().saturating_add(16));
    out.push('"');
    for ch in contents.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_ascii_control() => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Load `{name}.py` plus inject sibling `*.py` modules (for multi-file apps like `study/`).
fn load_python_app_sources(desc: &PythonAppDescriptor) -> Result<(String, String), String> {
    let main_name = format!("{}.py", desc.name);
    let main_src = std::fs::read_to_string(&desc.main_py)
        .map_err(|e| format!("failed to read {}: {e}", desc.main_py.display()))?;

    let mut prelude = String::new();
    let mut extras: Vec<_> = std::fs::read_dir(&desc.app_dir)
        .map_err(|e| format!("failed to read {}: {e}", desc.app_dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().map(|x| x == "py").unwrap_or(false)
                && e.file_name().to_string_lossy() != main_name
        })
        .collect();
    extras.sort_by_key(|e| e.file_name());

    for entry in extras {
        let stem = entry
            .path()
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if stem.is_empty() || stem == desc.name {
            continue;
        }
        let src = std::fs::read_to_string(entry.path())
            .map_err(|e| format!("failed to read {}: {e}", entry.path().display()))?;
        let quoted = escape_python_string_literal(&src);
        let _ = write!(
            &mut prelude,
            r#"import sys
__mod_src = {quoted}
__mod = sys.__class__("{stem}")
exec(compile(__mod_src, "{stem}.py", "exec"), __mod.__dict__)
sys.modules["{stem}"] = __mod

"#,
            quoted = quoted,
            stem = stem,
        );
    }

    let logical_path = desc.main_py.to_string_lossy().into_owned();
    Ok((format!("{prelude}{main_src}"), logical_path))
}

fn find_descriptor(name: &str, reserved_names: &[&str]) -> Option<PythonAppDescriptor> {
    let root = find_xos_project_root().ok()?;
    let discovered = discover_python_apps(&root, reserved_names).ok()?;
    discovered
        .apps
        .into_iter()
        .find(|a| a.name == name)
}

/// Build a [`PyApp`] from a discovered app name (reads from disk under `src/apps/`).
pub fn boxed_python_app(name: &str, reserved_names: &[&str]) -> Option<Box<dyn Application>> {
    let desc = find_descriptor(name, reserved_names)?;
    boxed_python_app_from_descriptor(&desc)
}

pub fn boxed_python_app_from_descriptor(desc: &PythonAppDescriptor) -> Option<Box<dyn Application>> {
    let (code, fname) = match load_python_app_sources(desc) {
        Ok(v) => v,
        Err(e) => {
            xos_core::print(&format!("❌ Failed to load python app {:?}:\n{e}", desc.name));
            return None;
        }
    };

    let print_cb = Arc::new(|s: &str| xos_core::print(s));
    let interpreter = Interpreter::with_init(Default::default(), |vm| {
        vm.add_native_module(
            "xos".to_owned(),
            Box::new(xos_python::xos_module::make_module),
        );
    });

    let (run_result, _output, app_instance, _) =
        execute_python_code(&interpreter, &code, &fname, None, Some(print_cb), &[]);

    if let Err(e) = run_result {
        xos_core::print(&format!(
            "❌ Failed to load python app {:?} ({}):\n{e}",
            desc.name, fname
        ));
        return None;
    }

    match app_instance {
        Some(app_inst) => Some(Box::new(PyApp::new(interpreter, app_inst))),
        None => {
            xos_core::print(&format!(
                "❌ {:?}: script did not register an xos.Application (call .run() at import or set __xos_app_instance__).",
                desc.name
            ));
            None
        }
    }
}
