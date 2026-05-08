use rustpython_vm::{
    builtins::{PyDict, PyList, PyModule, PyTuple},
    function::FuncArgs,
    PyRef, PyResult, VirtualMachine,
};
use std::io::Write;

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
    use winapi::um::wincon::{GetConsoleScreenBufferInfo, CONSOLE_SCREEN_BUFFER_INFO};

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

/// Real terminal dimensions from the controlling TTY (updates on resize).
#[cfg(unix)]
fn platform_size() -> Option<(i32, i32)> {
    use std::mem::MaybeUninit;
    use std::os::unix::io::AsRawFd;

    fn ioctl_winsize(fd: std::os::unix::io::RawFd) -> Option<(i32, i32)> {
        let mut ws = MaybeUninit::<libc::winsize>::uninit();
        // SAFETY: `TIOCGWINSZ` writes a `winsize` when `fd` refers to a TTY.
        let ret = unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, ws.as_mut_ptr()) };
        if ret != 0 {
            return None;
        }
        let ws = unsafe { ws.assume_init() };
        let cols = ws.ws_col as i32;
        let rows = ws.ws_row as i32;
        if cols > 0 && rows > 0 {
            Some((cols, rows))
        } else {
            None
        }
    }

    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let stdin = std::io::stdin();
    ioctl_winsize(stdout.as_raw_fd())
        .or_else(|| ioctl_winsize(stderr.as_raw_fd()))
        .or_else(|| ioctl_winsize(stdin.as_raw_fd()))
}

#[cfg(all(not(unix), not(target_os = "windows")))]
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

fn minecraft_color_to_ansi(code: char) -> Option<&'static str> {
    match code {
        '0' => Some("\x1b[30m"),
        '1' => Some("\x1b[34m"),
        '2' => Some("\x1b[32m"),
        '3' => Some("\x1b[36m"),
        '4' => Some("\x1b[31m"),
        '5' => Some("\x1b[35m"),
        '6' => Some("\x1b[33m"),
        '7' => Some("\x1b[37m"),
        '8' => Some("\x1b[90m"),
        '9' => Some("\x1b[94m"),
        'a' | 'A' => Some("\x1b[92m"),
        'b' | 'B' => Some("\x1b[96m"),
        'c' | 'C' => Some("\x1b[91m"),
        'd' | 'D' => Some("\x1b[95m"),
        'e' | 'E' => Some("\x1b[93m"),
        'f' | 'F' => Some("\x1b[97m"),
        'l' | 'L' => Some("\x1b[1m"),
        'n' | 'N' => Some("\x1b[4m"),
        'o' | 'O' => Some("\x1b[3m"),
        'm' | 'M' => Some("\x1b[9m"),
        'r' | 'R' => Some("\x1b[0m"),
        _ => None,
    }
}

fn get_width(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let (width, _) = terminal_size();
    Ok(vm.ctx.new_int(width).into())
}

fn get_height(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let (_, height) = terminal_size();
    Ok(vm.ctx.new_int(height).into())
}

/// Returns an xos.Tensor with shape (width, height, 2):
/// channel 0 = text char, channel 1 = minecraft color code char.
fn get_frame(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let (width, height) = terminal_size();
    let w = width.max(1) as usize;
    let h = height.max(1) as usize;

    let mut flat = Vec::with_capacity(w.saturating_mul(h).saturating_mul(2));
    for _x in 0..w {
        for _y in 0..h {
            flat.push(vm.ctx.new_str(" ").into());
            flat.push(vm.ctx.new_str("r").into());
        }
    }

    let tensor_data = vm.ctx.new_dict();
    tensor_data.set_item(
        "shape",
        vm.ctx
            .new_tuple(vec![
                vm.ctx.new_int(w).into(),
                vm.ctx.new_int(h).into(),
                vm.ctx.new_int(2).into(),
            ])
            .into(),
        vm,
    )?;
    tensor_data.set_item("dtype", vm.ctx.new_str("char").into(), vm)?;
    tensor_data.set_item("device", vm.ctx.new_str("cpu").into(), vm)?;
    tensor_data.set_item("_data", vm.ctx.new_list(flat).into(), vm)?;

    let tensor_ctor = vm
        .builtins
        .get_attr("Tensor", vm)
        .map_err(|_| vm.new_runtime_error("xos.Tensor is not available".to_string()))?;
    let tensor_data_obj: rustpython_vm::PyObjectRef = tensor_data.into();
    tensor_ctor.call((tensor_data_obj,), vm)
}

fn shape_from_obj(
    shape_obj: rustpython_vm::PyObjectRef,
    vm: &VirtualMachine,
) -> PyResult<(usize, usize, usize)> {
    if let Some(t) = shape_obj.downcast_ref::<PyTuple>() {
        let items = t.as_slice();
        if items.len() != 3 {
            return Err(vm.new_value_error(
                "terminal frame tensor shape must be (width, height, channels)".to_string(),
            ));
        }
        let w: usize = items[0].clone().try_into_value(vm)?;
        let h: usize = items[1].clone().try_into_value(vm)?;
        let c: usize = items[2].clone().try_into_value(vm)?;
        return Ok((w, h, c));
    }
    if let Some(l) = shape_obj.downcast_ref::<PyList>() {
        let items = l.borrow_vec();
        if items.len() != 3 {
            return Err(vm.new_value_error(
                "terminal frame tensor shape must be (width, height, channels)".to_string(),
            ));
        }
        let w: usize = items[0].clone().try_into_value(vm)?;
        let h: usize = items[1].clone().try_into_value(vm)?;
        let c: usize = items[2].clone().try_into_value(vm)?;
        return Ok((w, h, c));
    }
    Err(vm
        .new_type_error("terminal frame tensor shape must be a tuple/list of length 3".to_string()))
}

/// Render a terminal frame tensor.
fn set_frame(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let frame_obj = if let Some(o) = args.args.first() {
        o.clone()
    } else if let Some(o) = args.kwargs.get("frame") {
        o.clone()
    } else {
        return Err(vm.new_type_error("set_frame(frame) expects a tensor".to_string()));
    };

    let frame_data_obj = match vm.get_attribute_opt(frame_obj.clone(), "_data") {
        Ok(Some(d)) => d,
        _ => frame_obj,
    };
    let Some(frame_data) = frame_data_obj.downcast_ref::<PyDict>() else {
        return Err(
            vm.new_type_error("set_frame(frame): frame must be xos.Tensor-backed".to_string())
        );
    };

    let shape_obj = frame_data.get_item("shape", vm)?;
    let (width, height, channels) = shape_from_obj(shape_obj, vm)?;
    if channels < 2 {
        return Err(
            vm.new_value_error("terminal frame tensor requires at least 2 channels".to_string())
        );
    }
    if width == 0 || height == 0 {
        return Err(
            vm.new_value_error("terminal frame tensor width/height must be > 0".to_string())
        );
    }
    let (cur_w, cur_h) = terminal_size();
    let expected_w = cur_w.max(1) as usize;
    let expected_h = cur_h.max(1) as usize;
    if width != expected_w || height != expected_h {
        return Err(vm.new_value_error(format!(
            "terminal frame shape mismatch: got ({width}, {height}, {channels}), expected ({expected_w}, {expected_h}, >=2)"
        )));
    }

    let flat_obj = frame_data.get_item("_data", vm)?;
    let Some(flat_list) = flat_obj.downcast_ref::<PyList>() else {
        return Err(vm.new_type_error("terminal frame tensor _data must be a list".to_string()));
    };
    let flat_vec = flat_list.borrow_vec();

    let expected = width.saturating_mul(height).saturating_mul(channels);
    if flat_vec.len() < expected {
        return Err(vm.new_value_error(format!(
            "terminal frame tensor _data length mismatch: got {}, need at least {}",
            flat_vec.len(),
            expected
        )));
    }

    let mut out = String::new();
    out.push_str("\x1b[H\x1b[2J");

    for y in 0..height {
        let mut current_color = 'r';

        for x in 0..width {
            let base = (x.saturating_mul(height).saturating_add(y)).saturating_mul(channels);
            let ch = flat_vec
                .get(base)
                .and_then(|o| o.str(vm).ok())
                .and_then(|s| s.as_str().chars().next())
                .unwrap_or(' ');
            let code = flat_vec
                .get(base + 1)
                .and_then(|o| o.str(vm).ok())
                .and_then(|s| s.as_str().chars().next())
                .unwrap_or('r');
            if code != current_color {
                if let Some(ansi) = minecraft_color_to_ansi(code) {
                    out.push_str(ansi);
                } else {
                    out.push_str("\x1b[0m");
                }
                current_color = code;
            }
            out.push(ch);
        }

        out.push_str("\x1b[0m");
        if y + 1 < height {
            out.push('\n');
        }
    }

    let cursor_x = args
        .kwargs
        .get("cursor_x")
        .and_then(|o| o.clone().try_into_value::<usize>(vm).ok())
        .unwrap_or(0)
        .min(width.saturating_sub(1));
    let cursor_y = args
        .kwargs
        .get("cursor_y")
        .and_then(|o| o.clone().try_into_value::<usize>(vm).ok())
        .unwrap_or(height.saturating_sub(1))
        .min(height.saturating_sub(1));
    out.push_str(format!("\x1b[{};{}H", cursor_y + 1, cursor_x + 1).as_str());

    print!("{out}");
    let _ = std::io::stdout().flush();
    Ok(vm.ctx.none())
}

pub fn make_terminal_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.terminal", vm.ctx.new_dict(), None);
    let _ = module.set_attr("get_width", vm.new_function("get_width", get_width), vm);
    let _ = module.set_attr("get_height", vm.new_function("get_height", get_height), vm);
    let _ = module.set_attr("get_frame", vm.new_function("get_frame", get_frame), vm);
    let _ = module.set_attr("set_frame", vm.new_function("set_frame", set_frame), vm);
    module
}
