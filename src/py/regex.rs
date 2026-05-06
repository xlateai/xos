use regex::Regex;
use rustpython_vm::{PyObjectRef, PyRef, PyResult, VirtualMachine, builtins::PyModule, function::FuncArgs};

fn compile_regex(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() != 1 {
        return Err(vm.new_type_error(format!(
            "_compile() takes exactly 1 argument ({} given)",
            args_vec.len()
        )));
    }
    let pattern: String = args_vec[0].clone().try_into_value(vm)?;
    Regex::new(&pattern)
        .map_err(|e| vm.new_value_error(format!("invalid regex pattern: {e}")))?;
    Ok(vm.ctx.none())
}

fn match_regex(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let args_vec = args.args;
    if args_vec.len() != 2 {
        return Err(vm.new_type_error(format!(
            "_match() takes exactly 2 arguments ({} given)",
            args_vec.len()
        )));
    }
    let pattern: String = args_vec[0].clone().try_into_value(vm)?;
    let text: String = args_vec[1].clone().try_into_value(vm)?;
    let re =
        Regex::new(&pattern).map_err(|e| vm.new_value_error(format!("invalid regex pattern: {e}")))?;
    if let Some(caps) = re.captures(&text) {
        if let Some(m0) = caps.get(0) {
            if m0.start() != 0 {
                return Ok(vm.ctx.none());
            }
        } else {
            return Ok(vm.ctx.none());
        }
        let mut groups: Vec<PyObjectRef> = Vec::with_capacity(caps.len());
        for i in 0..caps.len() {
            if let Some(m) = caps.get(i) {
                groups.push(vm.ctx.new_str(m.as_str()).into());
            } else {
                groups.push(vm.ctx.none());
            }
        }
        return Ok(vm.ctx.new_list(groups).into());
    }
    Ok(vm.ctx.none())
}

pub fn make_regex_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.regex", vm.ctx.new_dict(), None);
    module
        .set_attr("_compile", vm.new_function("_compile", compile_regex), vm)
        .unwrap();
    module
        .set_attr("_match", vm.new_function("_match", match_regex), vm)
        .unwrap();

    let scope = vm.new_scope_with_builtins();
    let compile_fn = module.get_attr("_compile", vm).unwrap();
    scope.globals.set_item("_compile", compile_fn, vm).unwrap();
    let match_fn = module.get_attr("_match", vm).unwrap();
    scope.globals.set_item("_match", match_fn, vm).unwrap();

    let py_regex_code = r#"
class MatchResult:
    def __init__(self, groups):
        self._groups = groups

    def group(self, idx=0):
        i = int(idx)
        if i < 0 or i >= len(self._groups):
            raise IndexError("group index out of range")
        return self._groups[i]

class RegularExpression:
    def __init__(self, pattern):
        self.pattern = str(pattern)
        _compile(self.pattern)

    def match(self, text):
        groups = _match(self.pattern, str(text))
        if groups is None:
            return None
        return MatchResult(groups)

def compile(pattern):
    return RegularExpression(pattern)
"#;
    let _ = vm.run_code_string(scope.clone(), py_regex_code, "<xos_regex>".to_string());
    if let Ok(cls) = scope.globals.get_item("RegularExpression", vm) {
        module.set_attr("RegularExpression", cls, vm).unwrap();
    }
    if let Ok(cls) = scope.globals.get_item("MatchResult", vm) {
        module.set_attr("MatchResult", cls, vm).unwrap();
    }
    if let Ok(fn_obj) = scope.globals.get_item("compile", vm) {
        module.set_attr("compile", fn_obj, vm).unwrap();
    }
    module
}
