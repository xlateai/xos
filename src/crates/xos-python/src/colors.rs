use rustpython_vm::{builtins::PyModule, PyRef, VirtualMachine};

/// Resolve a palette name as exposed on `xos.color` (e.g. `WHITE`, `light_blue`, `gray`) to RGB.
pub use xos_core::named_colors::{lookup_xos_named_color_rgb, XOS_COLORS};

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
