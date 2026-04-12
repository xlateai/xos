use rustpython_vm::{PyRef, PyResult, VirtualMachine, builtins::PyModule, function::FuncArgs};

const DEFAULT_TERMINAL_WIDTH: i32 = 120;
const DEFAULT_TERMINAL_HEIGHT: i32 = 30;

fn env_size() -> Option<(i32, i32)> {
    let width = std::env::var("COLUMNS").ok()?.parse::<i32>().ok()?;
    let height = std::env::var("LINES").ok()?.parse::<i32>().ok()?;
    Some((width, height))
}

#[cfg(target_os = "windows")]
fn platform_size() -> Option<(i32, i32)> {
    use winapi::um::handleapi::INVALID_HANDLE_VALUE;
    use winapi::um::processenv::GetStdHandle;
    use winapi::um::winbase::STD_OUTPUT_HANDLE;
    use winapi::um::wincon::{CONSOLE_SCREEN_BUFFER_INFO, GetConsoleScreenBufferInfo};

    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return None;
        }

        let mut info: CONSOLE_SCREEN_BUFFER_INFO = std::mem::zeroed();
        if GetConsoleScreenBufferInfo(handle, &mut info) == 0 {
            return None;
        }

        let width = (info.srWindow.Right - info.srWindow.Left + 1) as i32;
        let height = (info.srWindow.Bottom - info.srWindow.Top + 1) as i32;
        Some((width, height))
    }
}

#[cfg(not(target_os = "windows"))]
fn platform_size() -> Option<(i32, i32)> {
    None
}

fn terminal_size() -> (i32, i32) {
    if let Some((w, h)) = platform_size() {
        return (w.max(40), h.max(10));
    }
    if let Some((w, h)) = env_size() {
        return (w.max(40), h.max(10));
    }
    (DEFAULT_TERMINAL_WIDTH, DEFAULT_TERMINAL_HEIGHT)
}

fn get_width(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let (width, _) = terminal_size();
    Ok(vm.ctx.new_int(width).into())
}

fn get_height(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let (_, height) = terminal_size();
    Ok(vm.ctx.new_int(height).into())
}

pub fn make_terminal_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.terminal", vm.ctx.new_dict(), None);
    let _ = module.set_attr("get_width", vm.new_function("get_width", get_width), vm);
    let _ = module.set_attr("get_height", vm.new_function("get_height", get_height), vm);
    module
}
