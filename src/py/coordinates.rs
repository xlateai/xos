//! `xos.coordinates` — axis tokens for per-edge layout (width / height / min/max side).

use rustpython_vm::{builtins::PyModule, PyRef, VirtualMachine};

const COORDS_INIT: &str = r#"
class _ViewportWidth:
    """Coefficient scales with framebuffer width (CSS ``vw``-style semantics)."""

    __slots__ = ()

    def __repr__(self):
        return "VIEWPORT_WIDTH"


VIEWPORT_WIDTH = _ViewportWidth()


class _ViewportHeight:
    """Coefficient scales with framebuffer height (CSS ``vh``-style semantics)."""

    __slots__ = ()

    def __repr__(self):
        return "VIEWPORT_HEIGHT"


VIEWPORT_HEIGHT = _ViewportHeight()


class _ViewportMaxDimension:
    """Coefficient scales with ``max(width, height)`` — largest viewport side."""

    __slots__ = ()

    def __repr__(self):
        return "VIEWPORT_MAX_DIMENSION"


VIEWPORT_MAX_DIMENSION = _ViewportMaxDimension()


class _ViewportMinDimension:
    """Coefficient scales with ``min(width, height)`` — smallest viewport side."""

    __slots__ = ()

    def __repr__(self):
        return "VIEWPORT_MIN_DIMENSION"


VIEWPORT_MIN_DIMENSION = _ViewportMinDimension()
"#;

/// Build `xos.coordinates` (singleton axis markers). Used by `xos.ui` layout.
pub fn make_coordinates_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.coordinates", vm.ctx.new_dict(), None);
    let scope = vm.new_scope_with_builtins();
    let _ = vm.run_code_string(
        scope.clone(),
        COORDS_INIT,
        "<xos.coordinates>".to_string(),
    );
    let vw = scope.globals.get_item("VIEWPORT_WIDTH", vm).unwrap();
    let vh = scope.globals.get_item("VIEWPORT_HEIGHT", vm).unwrap();
    let vmax = scope.globals.get_item("VIEWPORT_MAX_DIMENSION", vm).unwrap();
    let vmin = scope.globals.get_item("VIEWPORT_MIN_DIMENSION", vm).unwrap();
    module.set_attr("VIEWPORT_WIDTH", vw, vm).unwrap();
    module.set_attr("VIEWPORT_HEIGHT", vh, vm).unwrap();
    module.set_attr("VIEWPORT_MAX_DIMENSION", vmax, vm).unwrap();
    module.set_attr("VIEWPORT_MIN_DIMENSION", vmin, vm).unwrap();
    module
}
