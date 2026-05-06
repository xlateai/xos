use rustpython_vm::{PyRef, VirtualMachine, builtins::PyModule};

/// Resolve a palette name as exposed on `xos.color` (e.g. `WHITE`, `light_blue`, `gray`) to RGB.
pub fn lookup_xos_named_color_rgb(name: &str) -> Option<(u8, u8, u8)> {
    let key = name.trim();
    if key.is_empty() {
        return None;
    }
    for (snake, upper, rgb) in XOS_COLORS {
        if key.eq_ignore_ascii_case(snake) || key.eq_ignore_ascii_case(upper) {
            return Some(rgb);
        }
    }
    None
}

pub const XOS_COLORS: [(&str, &str, (u8, u8, u8)); 16] = [
    ("white", "WHITE", (255, 255, 255)),
    ("orange", "ORANGE", (249, 128, 29)),
    ("magenta", "MAGENTA", (199, 78, 189)),
    ("light_blue", "LIGHT_BLUE", (58, 179, 218)),
    ("yellow", "YELLOW", (254, 216, 61)),
    ("lime", "LIME", (128, 199, 31)),
    ("pink", "PINK", (243, 139, 170)),
    ("gray", "GRAY", (71, 79, 82)),
    ("light_gray", "LIGHT_GRAY", (157, 157, 151)),
    ("cyan", "CYAN", (22, 156, 156)),
    ("purple", "PURPLE", (137, 50, 184)),
    ("blue", "BLUE", (60, 68, 170)),
    ("brown", "BROWN", (131, 84, 50)),
    ("green", "GREEN", (94, 124, 22)),
    ("red", "RED", (176, 46, 38)),
    ("black", "BLACK", (0, 0, 0)),
];

pub fn make_color_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.color", vm.ctx.new_dict(), None);

    for (name, uppercase_name, (r, g, b)) in XOS_COLORS {
        let rgb = vm.ctx.new_tuple(vec![
            vm.ctx.new_int(r).into(),
            vm.ctx.new_int(g).into(),
            vm.ctx.new_int(b).into(),
        ]);
        module.set_attr(name, rgb.clone(), vm).unwrap();
        module.set_attr(uppercase_name, rgb, vm).unwrap();
    }

    module
}
