use rustpython_vm::{
    Interpreter, PyObjectRef, PyResult, AsObject, VirtualMachine, builtins::PyBaseExceptionRef,
};
use crate::engine::keyboard::shortcuts::ShortcutAction;
use crate::engine::{Application, EngineState, ScrollWheelUnit};
use crate::python_api::engine::py_engine_tls::{CallbackEngineStateGuard, TickEngineStateGuard};

/// Format a Python exception with traceback info
fn format_python_exception(vm: &VirtualMachine, py_exc: &PyBaseExceptionRef) -> String {
    let mut output = String::new();
    
    // Try to show traceback info if available
    if let Some(traceback) = py_exc.traceback() {
        output.push_str("Traceback (most recent call last):\n");
        
        // Use the debug format which should show file/line info
        let tb_str = format!("{:?}", traceback);
        if !tb_str.is_empty() && tb_str.len() < 500 {
            // Try to extract useful info from the debug string
            for line in tb_str.lines() {
                if line.contains("File") || line.contains("line") {
                    output.push_str("  ");
                    output.push_str(line.trim());
                    output.push('\n');
                }
            }
        }
    }
    
    // Get exception class name
    let class_name = py_exc.class().name().to_string();
    
    // Try to get the exception message
    let msg = vm.call_method(py_exc.as_object(), "__str__", ())
        .ok()
        .and_then(|result| result.str(vm).ok().map(|s| s.to_string()))
        .unwrap_or_default();
    
    // Add exception info
    if !msg.is_empty() {
        output.push_str(&format!("{}: {}", class_name, msg));
    } else {
        output.push_str(&class_name);
    }
    
    output
}

#[derive(Clone, Copy)]
pub(crate) enum RoutedPyEvent {
    MouseDown,
    MouseUp,
    MouseMove,
    Scroll {
        dx: f32,
        dy: f32,
        unit: ScrollWheelUnit,
    },
    KeyChar(char),
    Shortcut(ShortcutAction),
}

fn build_routed_event_dict(vm: &VirtualMachine, ev: RoutedPyEvent) -> PyResult {
    let d = vm.ctx.new_dict();
    match ev {
        RoutedPyEvent::MouseDown => {
            d.set_item("kind", vm.ctx.new_str("mouse_down").into(), vm)?;
        }
        RoutedPyEvent::MouseUp => {
            d.set_item("kind", vm.ctx.new_str("mouse_up").into(), vm)?;
        }
        RoutedPyEvent::MouseMove => {
            d.set_item("kind", vm.ctx.new_str("mouse_move").into(), vm)?;
        }
        RoutedPyEvent::Scroll { dx, dy, unit } => {
            d.set_item("kind", vm.ctx.new_str("scroll").into(), vm)?;
            d.set_item("dx", vm.ctx.new_float(dx as f64).into(), vm)?;
            d.set_item("dy", vm.ctx.new_float(dy as f64).into(), vm)?;
            let u = match unit {
                ScrollWheelUnit::Line => "line",
                ScrollWheelUnit::Pixel => "pixel",
            };
            d.set_item("unit", vm.ctx.new_str(u).into(), vm)?;
        }
        RoutedPyEvent::KeyChar(ch) => {
            d.set_item("kind", vm.ctx.new_str("key_char").into(), vm)?;
            d.set_item("char", vm.ctx.new_str(ch.to_string()).into(), vm)?;
        }
        RoutedPyEvent::Shortcut(sa) => {
            d.set_item("kind", vm.ctx.new_str("shortcut").into(), vm)?;
            let action = match sa {
                ShortcutAction::Copy => "copy",
                ShortcutAction::Cut => "cut",
                ShortcutAction::Paste => "paste",
                ShortcutAction::SelectAll => "select_all",
                ShortcutAction::Undo => "undo",
                ShortcutAction::Redo => "redo",
            };
            d.set_item("action", vm.ctx.new_str(action).into(), vm)?;
        }
    }
    Ok(d.into())
}

fn try_dispatch_python_on_events(
    vm: &VirtualMachine,
    app_instance: &PyObjectRef,
    state: &mut EngineState,
    ev: RoutedPyEvent,
) {
    let Ok(Some(cb)) = vm.get_attribute_opt(app_instance.clone(), "on_events") else {
        return;
    };
    if !cb.is_callable() {
        return;
    }
    let dict = match build_routed_event_dict(vm, ev) {
        Ok(o) => o,
        Err(e) => {
            eprintln!(
                "Failed to build routed _xos_event dict:\n{}",
                format_python_exception(vm, &e)
            );
            return;
        }
    };
    let _evt_store = app_instance.set_attr("_xos_event", dict, vm);
    let _guard = CallbackEngineStateGuard::install(state);
    if let Err(e) = vm.call_method(app_instance, "on_events", ()) {
        eprintln!(
            "Python on_events error:\n{}",
            format_python_exception(vm, &e)
        );
    }
    let _clear = app_instance.set_attr("_xos_event", vm.ctx.none(), vm);
}

pub const APPLICATION_CLASS_CODE: &str = r#"
def _tensor_unflatten(flat, shape):
    """Row-major flat list -> nested list for the given ``shape``."""
    shape = tuple(shape)
    if len(shape) == 0:
        if len(flat) != 1:
            raise ValueError("scalar tensor expects one element")
        return flat[0]
    if len(shape) == 1:
        n = shape[0]
        if len(flat) != n:
            raise ValueError("tensor list(): length mismatch with shape")
        return list(flat)
    d0 = shape[0]
    rest = shape[1:]
    chunk = 1
    for s in rest:
        chunk *= s
    if len(flat) != d0 * chunk:
        raise ValueError("tensor list(): length mismatch with shape")
    return [_tensor_unflatten(flat[i * chunk : (i + 1) * chunk], rest) for i in range(d0)]

def _nested_list_to_tuple(nested):
    if isinstance(nested, list):
        return tuple(_nested_list_to_tuple(x) for x in nested)
    return nested

class Tensor:
    """xos.Tensor — dict-backed tensor (``shape``, ``dtype``, ``_data``) or flat list + optional ``shape``."""
    def __init__(self, data, shape=None):
        if isinstance(data, dict):
            self._data = data
        else:
            flat = list(data)
            sh = shape if shape is not None else (len(flat),)
            self._data = {
                "shape": tuple(sh),
                "dtype": "float32",
                "device": "cpu",
                "_data": flat,
            }

    def __iter__(self):
        d = self._data.get("_data")
        if isinstance(d, list):
            return iter(d)
        return iter([])

    def __len__(self):
        d = self._data.get("_data")
        return len(d) if isinstance(d, list) else 0

    def _getitem_int_index(self, key):
        """Row-major integer indexing: ``t[i]``, ``t[i,j]``, … partial views or a scalar."""
        shape = tuple(self.shape)
        flat = self._data["_data"]
        if isinstance(key, int):
            indices = (key,)
        else:
            indices = key
        offset = 0
        dim = 0
        for k in indices:
            if dim >= len(shape):
                raise IndexError("too many indices for tensor")
            kk = int(k)
            if kk < 0:
                kk += shape[dim]
            if kk < 0 or kk >= shape[dim]:
                raise IndexError("tensor index out of range")
            stride = 1
            for s in shape[dim + 1 :]:
                stride *= s
            offset += kk * stride
            dim += 1
        remaining = shape[dim:]
        n = 1
        for s in remaining:
            n *= s
        subflat = flat[offset : offset + n]
        if len(remaining) == 0:
            return subflat[0]
        return self._wrap_vals(remaining, subflat)
    
    def __getitem__(self, key):
        if isinstance(key, slice) and key == slice(None, None, None):
            # Return the underlying data dict for full slice access
            return self._data
        # N-dimensional integer indexing (e.g. y[0], y[0, 0], y[0, 0, 1])
        if "_data" in self._data and isinstance(self._data["_data"], list):
            if type(key) is int:
                return self._getitem_int_index(key)
            if (
                isinstance(key, tuple)
                and len(key) > 0
                and all(type(x) is int for x in key)
            ):
                return self._getitem_int_index(key)
        if isinstance(key, tuple) and len(key) == 2:
            a, b = key
            shape = tuple(self.shape)
            flat = self._data["_data"]
            if isinstance(a, slice) and a.start is None and a.stop is None and a.step is None and b is None:
                if len(shape) == 1:
                    n = shape[0]
                    return self._wrap_vals((n, 1), flat)
                if len(shape) == 2:
                    r, c = shape[0], shape[1]
                    return self._wrap_vals((r, c, 1), flat)
            if isinstance(a, slice) and a.start is None and a.stop is None and a.step is None and isinstance(b, int):
                if len(shape) == 2:
                    rows, cols = shape[0], shape[1]
                    col = b
                    out = [flat[i * cols + col] for i in range(rows)]
                    return self._wrap_vals((rows,), out)
        if isinstance(key, Tensor):
            return self._gather_rows(key)
        return self._data[key]

    def _wrap_vals(self, shape, values):
        return Tensor({
            "shape": tuple(shape),
            "dtype": self.dtype,
            "device": self._data.get("device", "cpu"),
            "_data": values,
        })

    def reshape(self, new_shape):
        flat = self._data["_data"]
        prod = 1
        for d in new_shape:
            prod *= d
        if prod != len(flat):
            raise ValueError("reshape size mismatch")
        return self._wrap_vals(tuple(new_shape), flat)

    def _gather_rows(self, idx_tensor):
        idx_flat = idx_tensor._data["_data"]
        shape = tuple(self.shape)
        flat = self._data["_data"]
        if len(shape) != 2:
            raise NotImplementedError("gather only supports 2D tensors")
        r, c = shape[0], shape[1]
        out = []
        for ii in idx_flat:
            i = int(ii)
            if i < 0 or i >= r:
                raise IndexError("index out of range")
            base = i * c
            for j in range(c):
                out.append(flat[base + j])
        return self._wrap_vals((len(idx_flat), c), out)

    def __setitem__(self, key, value):
        if isinstance(key, slice) and key == slice(None, None, None):
            # Full slice assignment
            # Check if value is a sentinel dict indicating direct fill already happened
            if isinstance(value, dict) and value.get('_direct_fill', False):
                # Data already written directly to buffer by Rust - ZERO COPY! Do nothing.
                return
            # Call Rust function to fill buffer (handles lists and Tensor)
            import xos
            xos.rasterizer._fill_buffer(self._data, value)
        else:
            self._data[key] = value

    def _wrap_like_self(self, values):
        return Tensor({
            "shape": self.shape,
            "dtype": self.dtype,
            "device": self._data.get("device", "cpu"),
            "_data": values,
        })

    def _binary_op(self, other, op):
        left = self._data.get("_data", [])
        lshape = tuple(self.shape)
        if isinstance(other, Tensor):
            right = other._data.get("_data", [])
            rshape = tuple(other.shape)
            if isinstance(right, list):
                if len(lshape) == 2 and len(rshape) == 2 and lshape[1] == 2 and rshape[0] == 1 and rshape[1] == 2 and len(right) == 2:
                    w0, w1 = right[0], right[1]
                    n = lshape[0]
                    out = []
                    for i in range(n):
                        out.append(op(left[2 * i], w0))
                        out.append(op(left[2 * i + 1], w1))
                    return self._wrap_vals(lshape, out)
                if len(left) != len(right):
                    raise ValueError(f"shape mismatch for op: {len(left)} vs {len(right)} elements")
                out = [op(a, b) for a, b in zip(left, right)]
                return self._wrap_like_self(out)
        elif isinstance(other, dict) and "_data" in other:
            right = other["_data"]
        else:
            right = other

        if isinstance(right, list):
            if len(left) != len(right):
                raise ValueError(f"shape mismatch for op: {len(left)} vs {len(right)} elements")
            out = [op(a, b) for a, b in zip(left, right)]
        else:
            out = [op(a, right) for a in left]
        return self._wrap_like_self(out)

    def _cmp_broadcast(self, other, cmp_fn):
        left = self._data["_data"]
        lshape = tuple(self.shape)
        if isinstance(other, Tensor):
            right = other._data["_data"]
            rshape = tuple(other.shape)
            if lshape == rshape:
                return self._wrap_vals(lshape, [cmp_fn(a, b) for a, b in zip(left, right)])
            if len(lshape) == 2 and len(rshape) == 2 and lshape[0] == rshape[0] and lshape[1] == 2 and rshape[1] == 1:
                n = lshape[0]
                out = []
                for i in range(n):
                    rv = right[i]
                    out.append(cmp_fn(left[2 * i], rv))
                    out.append(cmp_fn(left[2 * i + 1], rv))
                return self._wrap_vals(lshape, out)
        if isinstance(other, (int, float)):
            return self._wrap_vals(lshape, [cmp_fn(a, other) for a in left])
        raise TypeError("unsupported comparison operand")

    def __lt__(self, other):
        return self._cmp_broadcast(other, lambda a, b: 1.0 if a < b else 0.0)

    def __gt__(self, other):
        return self._cmp_broadcast(other, lambda a, b: 1.0 if a > b else 0.0)

    def __or__(self, other):
        if isinstance(other, Tensor):
            la = self._data["_data"]
            lb = other._data["_data"]
            if len(la) != len(lb):
                raise ValueError("or shape mismatch")
            return self._wrap_vals(self.shape, [1.0 if (a != 0.0 or b != 0.0) else 0.0 for a, b in zip(la, lb)])
        raise TypeError("unsupported or operand")

    def __neg__(self):
        return self._wrap_vals(self.shape, [-a for a in self._data["_data"]])

    def __add__(self, other):
        return self._binary_op(other, lambda a, b: a + b)

    def __radd__(self, other):
        return self.__add__(other)

    def __sub__(self, other):
        return self._binary_op(other, lambda a, b: a - b)

    def __rsub__(self, other):
        if isinstance(other, Tensor):
            return other.__sub__(self)
        left = self._data.get("_data", [])
        return self._wrap_like_self([other - a for a in left])

    def __mul__(self, other):
        return self._binary_op(other, lambda a, b: a * b)

    def __rmul__(self, other):
        return self.__mul__(other)
    
    @property
    def shape(self):
        return self._data.get('shape', ())
    
    @property
    def dtype(self):
        return self._data.get('dtype', 'unknown')

    def to(self, dtype):
        """Return a new tensor converted to ``dtype`` using flat elementwise casting."""
        if "_data" not in self._data or not isinstance(self._data["_data"], list):
            raise TypeError("tensor has no element data for to()")

        if isinstance(dtype, str):
            target = dtype.strip().lower()
        elif hasattr(dtype, "name"):
            target = str(dtype.name).strip().lower()
        else:
            raise TypeError("dtype must be a dtype object or string")

        alias = {
            "u8": "uint8",
            "byte": "uint8",
            "f32": "float32",
            "float": "float32",
            "i32": "int32",
            "int": "int32",
            "i64": "int64",
        }
        target = alias.get(target, target)

        src = self._data["_data"]
        if target == "uint8":
            out = []
            for v in src:
                iv = int(v)
                if iv < 0:
                    iv = 0
                elif iv > 255:
                    iv = 255
                out.append(iv)
        elif target in ("int8", "int16", "int32", "int64", "uint16", "uint32", "uint64"):
            out = [int(v) for v in src]
        elif target in ("float16", "float32", "float64"):
            out = [float(v) for v in src]
        else:
            raise ValueError(f"unsupported dtype for to(): {target}")

        return Tensor({
            "shape": tuple(self.shape),
            "dtype": target,
            "device": self._data.get("device", "cpu"),
            "_data": out,
        })
    
    def size(self):
        """Return the total number of elements (product of all dimensions)"""
        shape = self.shape
        if not shape:
            return 0
        size = 1
        for dim in shape:
            size *= dim
        return size

    def list(self):
        """Nested Python lists with the same structure as ``shape`` (row-major)."""
        if "_data" not in self._data:
            raise TypeError("tensor has no element data for list()")
        flat = self._data["_data"]
        shape = tuple(self.shape)
        return _tensor_unflatten(flat, shape)

    def tuple(self):
        """Same as ``list()`` but with tuples at each nesting level."""
        return _nested_list_to_tuple(self.list())
    
    def __str__(self):
        # For regular tensors (not frame tensors), compute statistics
        if '_data' in self._data:
            data_list = self._data['_data']
            if data_list and len(data_list) > 0:
                min_val = min(data_list)
                max_val = max(data_list)
                mean_val = sum(data_list) / len(data_list)
                return f"xos.Tensor(shape={self.shape}, dtype={self.dtype}, min={min_val:.3f}, max={max_val:.3f}, mean={mean_val:.3f})"
            return f"xos.Tensor(shape={self.shape}, dtype={self.dtype}, empty)"
        # For frame tensors (no _data field)
        return f"xos.Tensor(shape={self.shape}, dtype=u8)"
    
    def __repr__(self):
        return self.__str__()

class EngineState:
    """Snapshot of engine context for Python ``Application.on_events`` (attributes set each call)."""

class Frame:
    """Wrapper to make frame dict behave like an object with methods"""
    def __init__(self, data):
        self._data = data
        self._tensor = Tensor(data.get('tensor', {}))
    
    @property
    def tensor(self):
        """CPU RGBA frame tensor (``xos.Tensor``) with slice assignment."""
        return self._tensor
    
    def get_width(self):
        return self._data['width']
    
    def get_height(self):
        return self._data['height']
    
    def __getitem__(self, key):
        return self._data[key]
    
    def __setitem__(self, key, value):
        self._data[key] = value

    def clear(self, *color):
        """
        Clear the frame with an RGB or RGBA color.
        Accepts:
          - frame.clear()                      -> black
          - frame.clear((r, g, b))            -> alpha defaults to 255
          - frame.clear((r, g, b, a))
          - frame.clear(r, g, b)
          - frame.clear(r, g, b, a)
        """
        import xos

        if len(color) == 0:
            rgba = (0, 0, 0, 255)
        elif len(color) == 1 and isinstance(color[0], tuple):
            if len(color[0]) == 3:
                rgba = (color[0][0], color[0][1], color[0][2], 255)
            elif len(color[0]) == 4:
                rgba = color[0]
            else:
                raise TypeError("frame.clear color tuple must be RGB or RGBA")
        elif len(color) == 3:
            rgba = (color[0], color[1], color[2], 255)
        elif len(color) == 4:
            rgba = (color[0], color[1], color[2], color[3])
        else:
            raise TypeError("frame.clear accepts (), RGB, or RGBA")

        # Standalone preview frames carry _xos_viewport_id. Only bind/unbind here when
        # there is no active tick-owned context.
        vid = self._data.get("_xos_viewport_id")
        bound = False
        if vid is not None and not xos.frame._has_context():
            w = int(self._data["width"])
            h = int(self._data["height"])
            xos.frame._begin_standalone(int(vid), w, h)
            bound = True
        try:
            xos.rasterizer.fill(self, rgba)
        finally:
            if bound:
                xos.frame._end_standalone()

class Application:
    """Base class for xos applications. Extend this class and implement __init__() and tick().

    Routed input sets ``self._xos_event`` (a dict with ``kind``, etc.) before ``on_events()`` runs and
    clears it only after your handler returns, so every component sees the same event in one call.
    Kinds include mouse, scroll, ``key_char``, and desktop ``shortcut`` (e.g. Cmd/Ctrl+C/V/X/A).
    Conventional order is ``self.keyboard.on_events(self)`` then ``self.text.on_events(self)`` for
    pointer, ``key_char``, and shortcuts alike — no special cases per event type are required.
    """
    
    def __init__(self, headless=None):
        import builtins
        next_id = int(getattr(builtins, "__xos_next_viewport_id__", 0))
        builtins.__xos_next_viewport_id__ = next_id + 1
        self._xos_viewport_id = next_id
        self._xos_initialized = True
        self._xos_engine_bound = False
        self.mouse = None  # Overwritten by engine in run(); standalone set below
        self.fps = 0.0  # Frames per second derived from timestep
        self.dt = 0.0  # Last frame delta time in seconds (same source as engine timestep)
        self.t = 0  # Tick index: 0 on first tick(), then increments after each tick completes
        # F3 scale as percent/100 (0.25..5.0 for 25–500%); default 1.0 at 100%.
        # Engine syncs `xos_scale` each tick.
        self.xos_scale = 1.0
        self._xos_standalone_width = 800
        self._xos_standalone_height = 600
        self._xos_last_tick_time = None
        self._xos_ticks_completed = 0
        if headless is not None:
            self.headless = bool(headless)

        # Empty standalone framebuffer before first tick() or run(); enables frame.clear etc. in __init__.
        self._xos_init_standalone_frame()

    def _xos_init_standalone_frame(self):
        """Build a CPU frame + rasterizer context for standalone use (before tick/run)."""
        import xos

        if getattr(self, "_xos_engine_bound", False):
            return
        w = int(getattr(self, "_xos_standalone_width", 800))
        h = int(getattr(self, "_xos_standalone_height", 600))
        fd = xos.frame._begin_standalone(int(self._xos_viewport_id), w, h)
        fd["_xos_viewport_id"] = int(self._xos_viewport_id)
        self.frame = Frame(fd)
        self.mouse = {"x": 0.0, "y": 0.0, "is_left_clicking": False, "is_right_clicking": False}
        xos.rasterizer.fill(self.frame, (0, 0, 0, 255))
        xos.frame._end_standalone()

    @property
    def scale(self):
        """F3 UI scale as percent/100 (100% → 1.0, 25–500% → 0.25–5.0). Updated each tick."""
        return float(getattr(self, "xos_scale", 1.0))

    @classmethod
    def __init_subclass__(cls, **kwargs):
        super().__init_subclass__(**kwargs)
        user_tick = cls.__dict__.get("tick")
        if user_tick is None:
            return

        def _wrapped_tick(self, *args, **kw):
            self._xos_pre_tick()
            try:
                return user_tick(self, *args, **kw)
            finally:
                self._xos_post_tick()

        cls.tick = _wrapped_tick

    def _xos_pre_tick(self):
        import xos, time
        if not getattr(self, "_xos_initialized", False):
            raise RuntimeError("xos.Application.__init__() was not called. Call super().__init__() first.")

        # Engine-driven mode (normal app.run lifecycle): engine owns frame context.
        if getattr(self, "_xos_engine_bound", False):
            self.pre_tick()
            return

        # In standalone preview mode, follow live preview window size and F3 scale.
        if not bool(getattr(self, "headless", False)):
            ws = xos.frame._standalone_window_size(self._xos_viewport_id)
            if ws is not None:
                w, h = ws
                self._xos_standalone_width = int(max(1, w))
                self._xos_standalone_height = int(max(1, h))
            sp = xos.frame._standalone_ui_scale(self._xos_viewport_id)
            if sp is not None:
                self.xos_scale = float(sp)

        # Standalone timing (manual Python-driven tick loop).
        now = time.perf_counter()
        last = getattr(self, "_xos_last_tick_time", None)
        if last is None:
            dt = 1.0 / 60.0
        else:
            dt = max(1e-5, now - last)
        self._xos_last_tick_time = now
        self.dt = dt
        self.fps = 1.0 / dt
        self.t = int(getattr(self, "_xos_ticks_completed", 0))

        # Standalone mode (manual Python-driven tick loop): create temporary frame context.
        frame_dict = xos.frame._begin_standalone(
            int(getattr(self, "_xos_viewport_id", 0)),
            int(getattr(self, "_xos_standalone_width", 800)),
            int(getattr(self, "_xos_standalone_height", 600)),
        )
        frame_dict["_xos_viewport_id"] = int(getattr(self, "_xos_viewport_id", 0))
        self.frame = Frame(frame_dict)
        self.mouse = {"x": 0.0, "y": 0.0, "is_left_clicking": False, "is_right_clicking": False}
        self.pre_tick()

    def _xos_post_tick(self):
        import xos
        try:
            self.post_tick()
        finally:
            if not getattr(self, "_xos_engine_bound", False):
                if not bool(getattr(self, "headless", False)):
                    xos.frame._present_standalone(int(getattr(self, "_xos_viewport_id", 0)))
                xos.frame._end_standalone()
                self._xos_ticks_completed = int(getattr(self, "_xos_ticks_completed", 0)) + 1
    
    def get_width(self):
        """Get the current frame width"""
        return self.frame.get_width()
    
    def get_height(self):
        """Get the current frame height"""
        return self.frame.get_height()
    
    def tick(self):
        """Called every frame. Override this method."""
        raise NotImplementedError("Subclasses must implement tick()")

    def pre_tick(self):
        """Called before each tick() in both run() and standalone tick() modes."""
        pass

    def post_tick(self):
        """Called after each tick() in both run() and standalone tick() modes."""
        pass
    
    def on_mouse_down(self, x, y):
        """Called when mouse is clicked. Override this method (optional)."""
        pass
    
    def on_mouse_up(self, x, y):
        """Called when mouse is released. Override this method (optional)."""
        pass
    
    def on_mouse_move(self, x, y):
        """Called when mouse moves. Override this method (optional)."""
        pass

    def on_scroll(self, dx, dy):
        """Called on mouse wheel / trackpad scroll. Override (optional)."""
        pass
    
    def on_screen_size_change(self, width, height):
        """Called when screen size changes. Override this method (optional)."""
        pass
    
    def run(self):
        """Run the application with the xos engine."""
        # Store self in builtins so Rust can find it from any scope
        import builtins
        builtins.__xos_app_instance__ = self
"#;

/// PyApp wraps a Python Application instance and implements the Rust Application trait
pub struct PyApp {
    interpreter: Interpreter,
    app_instance: Option<PyObjectRef>,
    /// Number of `tick()` calls that have fully finished (starts at 0; incremented after each tick).
    ticks_completed: u64,
}

impl PyApp {
    pub fn new(interpreter: Interpreter, app_instance: PyObjectRef) -> Self {
        Self {
            interpreter,
            app_instance: Some(app_instance),
            ticks_completed: 0,
        }
    }
}

impl Application for PyApp {
    fn setup(&mut self, state: &mut EngineState) -> Result<(), String> {
        if let Some(ref app_instance) = self.app_instance {
            self.interpreter.enter(|vm| {
                // Create Python frame object from engine state
                let frame_dict = crate::python_api::engine::py_bindings::create_py_frame_state(vm, &mut state.frame)
                    .map_err(|e| format!("Failed to create frame object: {:?}", e))?;
                
                if let Ok(wrapper_class) = vm.builtins.get_attr("Frame", vm) {
                    // Use the newer call API instead of deprecated invoke
                    if let Ok(frame_obj) = wrapper_class.call((frame_dict.clone(),), vm) {
                        app_instance.set_attr("frame", frame_obj, vm)
                            .map_err(|e| format!("Failed to set frame attribute: {:?}", e))?;
                    } else {
                        // Fallback: just use the dict directly
                        app_instance.set_attr("frame", frame_dict, vm)
                            .map_err(|e| format!("Failed to set frame attribute: {:?}", e))?;
                    }
                } else {
                    // Fallback: just use the dict directly
                    app_instance.set_attr("frame", frame_dict, vm)
                        .map_err(|e| format!("Failed to set frame attribute: {:?}", e))?;
                }
                
                // Create mouse object
                let mouse_dict = vm.ctx.new_dict();
                mouse_dict.set_item("x", vm.ctx.new_float(state.mouse.x as f64).into(), vm)
                    .map_err(|e| format!("Mouse x error: {:?}", e))?;
                mouse_dict.set_item("y", vm.ctx.new_float(state.mouse.y as f64).into(), vm)
                    .map_err(|e| format!("Mouse y error: {:?}", e))?;
                mouse_dict.set_item("is_left_clicking", vm.ctx.new_bool(state.mouse.is_left_clicking).into(), vm)
                    .map_err(|e| format!("Mouse clicking error: {:?}", e))?;
                mouse_dict.set_item("is_right_clicking", vm.ctx.new_bool(state.mouse.is_right_clicking).into(), vm)
                    .map_err(|e| format!("Mouse right click error: {:?}", e))?;
                
                app_instance.set_attr("mouse", mouse_dict, vm)
                    .map_err(|e| format!("Failed to set mouse attribute: {:?}", e))?;
                app_instance.set_attr("_xos_engine_bound", vm.ctx.new_bool(true), vm)
                    .map_err(|e| format!("Failed to set _xos_engine_bound attribute: {:?}", e))?;

                // Seed timing field so Python can read it in setup/tick.
                let timestep = state.delta_time_seconds.max(1e-5) as f64;
                app_instance.set_attr("dt", vm.ctx.new_float(timestep), vm)
                    .map_err(|e| format!("Failed to set dt attribute: {:?}", e))?;
                app_instance.set_attr("fps", vm.ctx.new_float(1.0 / timestep), vm)
                    .map_err(|e| format!("Failed to set fps attribute: {:?}", e))?;
                app_instance.set_attr("t", vm.ctx.new_int(0usize), vm)
                    .map_err(|e| format!("Failed to set t attribute: {:?}", e))?;
                app_instance.set_attr("xos_scale", vm.ctx.new_float(state.ui_scale_percent as f64 / 100.0), vm)
                    .map_err(|e| format!("Failed to set xos_scale attribute: {:?}", e))?;
                
                Ok(())
            })
        } else {
            Err("No Python app instance".to_string())
        }
    }

    fn tick(&mut self, state: &mut EngineState) {
        if let Some(app_instance) = self.app_instance.clone() {
            // Set the frame buffer context for the rasterizer
            let shape = state.frame.shape();
            let width = shape[1];
            let height = shape[0];
            let buffer = state.frame.buffer_mut();
            crate::python_api::rasterizer::set_frame_buffer_context(buffer, width, height);

            let tick_index = self.ticks_completed;
            let mut tick_failed = false;
            
            self.interpreter.enter(|vm| {
                // Require subclasses to call super().__init__() so base fields exist.
                let initialized_ok = match vm.get_attribute_opt(app_instance.clone(), "_xos_initialized") {
                    Ok(Some(flag_obj)) => flag_obj.clone().try_into_value::<bool>(vm).unwrap_or(false),
                    Ok(None) | Err(_) => false,
                };
                if !initialized_ok {
                    eprintln!(
                        "Python app init error:\nRuntimeError: xos.Application.__init__() was not called. \
Call super().__init__() in your app __init__ before using tick()."
                    );
                    tick_failed = true;
                    return;
                }

                // Update frame data before calling tick
                if let Ok(Some(frame_obj)) = vm.get_attribute_opt(app_instance.clone(), "frame") {
                    let _ = crate::python_api::engine::py_bindings::update_py_frame_state(vm, frame_obj.clone(), &mut state.frame);
                    
                    // Update mouse data
                    let mouse_dict = vm.ctx.new_dict();
                    let _ = mouse_dict.set_item("x", vm.ctx.new_float(state.mouse.x as f64).into(), vm);
                    let _ = mouse_dict.set_item("y", vm.ctx.new_float(state.mouse.y as f64).into(), vm);
                    let _ = mouse_dict.set_item("is_left_clicking", vm.ctx.new_bool(state.mouse.is_left_clicking).into(), vm);
                    let _ = mouse_dict.set_item("is_right_clicking", vm.ctx.new_bool(state.mouse.is_right_clicking).into(), vm);
                    let _ = app_instance.set_attr("mouse", mouse_dict, vm);

                    // Expose timestep and FPS directly to Python app.
                    let timestep = state.delta_time_seconds.max(1e-5) as f64;
                    let _ = app_instance.set_attr("dt", vm.ctx.new_float(timestep), vm);
                    let _ = app_instance.set_attr("fps", vm.ctx.new_float(1.0 / timestep), vm);

                    // Tick counter: value during tick() is N ticks completed so far (0 on first tick).
                    let _ = app_instance.set_attr("t", vm.ctx.new_int(tick_index as usize), vm);
                    let _ = app_instance.set_attr("xos_scale", vm.ctx.new_float(state.ui_scale_percent as f64 / 100.0), vm);
                    
                    // Call tick (xos.ui may call into Rust with `TickEngineStateGuard` active)
                    let _tls_guard = TickEngineStateGuard::install(state);
                    if let Err(e) = vm.call_method(&app_instance, "tick", ()) {
                        let error_msg = format_python_exception(vm, &e);
                        eprintln!("Python tick error:\n{}", error_msg);
                        tick_failed = true;
                    }
                }
            });

            if tick_failed {
                // Stop ticking this Python app after the first runtime error.
                self.app_instance = None;
                #[cfg(not(target_arch = "wasm32"))]
                crate::engine::native_engine::request_exit();
                eprintln!("Python app execution stopped after tick error.");
            } else {
                self.ticks_completed = self.ticks_completed.saturating_add(1);
            }
            
            // Clear the frame buffer context after tick
            crate::python_api::rasterizer::clear_frame_buffer_context();
        }
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        let shape = state.frame.shape();
        let _keyboard_hit = state.keyboard.onscreen.on_mouse_down(
            state.mouse.x,
            state.mouse.y,
            shape[1] as f32,
            shape[0] as f32,
        );
        if let Some(ref app_instance) = self.app_instance {
            self.interpreter.enter(|vm| {
                let x = state.mouse.x;
                let y = state.mouse.y;
                let mouse_dict = vm.ctx.new_dict();
                let _ = mouse_dict.set_item("x", vm.ctx.new_float(x as f64).into(), vm);
                let _ = mouse_dict.set_item("y", vm.ctx.new_float(y as f64).into(), vm);
                let _ = mouse_dict.set_item(
                    "is_left_clicking",
                    vm.ctx.new_bool(state.mouse.is_left_clicking).into(),
                    vm,
                );
                let _ = mouse_dict.set_item(
                    "is_right_clicking",
                    vm.ctx.new_bool(state.mouse.is_right_clicking).into(),
                    vm,
                );
                let _ = app_instance.set_attr("mouse", mouse_dict, vm);
                let _ = vm.call_method(app_instance, "on_mouse_down", (x, y));
                try_dispatch_python_on_events(vm, app_instance, state, RoutedPyEvent::MouseDown);
            });
        }
    }

    fn on_mouse_up(&mut self, state: &mut EngineState) {
        state.keyboard.onscreen.on_mouse_up();
        if let Some(ref app_instance) = self.app_instance {
            self.interpreter.enter(|vm| {
                let x = state.mouse.x;
                let y = state.mouse.y;
                let mouse_dict = vm.ctx.new_dict();
                let _ = mouse_dict.set_item("x", vm.ctx.new_float(x as f64).into(), vm);
                let _ = mouse_dict.set_item("y", vm.ctx.new_float(y as f64).into(), vm);
                let _ = mouse_dict.set_item(
                    "is_left_clicking",
                    vm.ctx.new_bool(state.mouse.is_left_clicking).into(),
                    vm,
                );
                let _ = mouse_dict.set_item(
                    "is_right_clicking",
                    vm.ctx.new_bool(state.mouse.is_right_clicking).into(),
                    vm,
                );
                let _ = app_instance.set_attr("mouse", mouse_dict, vm);
                let _ = vm.call_method(app_instance, "on_mouse_up", (x, y));
                try_dispatch_python_on_events(vm, app_instance, state, RoutedPyEvent::MouseUp);
            });
        }
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        if let Some(ref app_instance) = self.app_instance {
            self.interpreter.enter(|vm| {
                let x = state.mouse.x;
                let y = state.mouse.y;
                let mouse_dict = vm.ctx.new_dict();
                let _ = mouse_dict.set_item("x", vm.ctx.new_float(x as f64).into(), vm);
                let _ = mouse_dict.set_item("y", vm.ctx.new_float(y as f64).into(), vm);
                let _ = mouse_dict.set_item(
                    "is_left_clicking",
                    vm.ctx.new_bool(state.mouse.is_left_clicking).into(),
                    vm,
                );
                let _ = mouse_dict.set_item(
                    "is_right_clicking",
                    vm.ctx.new_bool(state.mouse.is_right_clicking).into(),
                    vm,
                );
                let _ = app_instance.set_attr("mouse", mouse_dict, vm);
                let _ = vm.call_method(app_instance, "on_mouse_move", (x, y));
                try_dispatch_python_on_events(vm, app_instance, state, RoutedPyEvent::MouseMove);
            });
        }
    }

    fn on_scroll(&mut self, state: &mut EngineState, dx: f32, dy: f32, unit: ScrollWheelUnit) {
        if let Some(ref app_instance) = self.app_instance {
            self.interpreter.enter(|vm| {
                let mouse_dict = vm.ctx.new_dict();
                let _ = mouse_dict.set_item("x", vm.ctx.new_float(state.mouse.x as f64).into(), vm);
                let _ = mouse_dict.set_item("y", vm.ctx.new_float(state.mouse.y as f64).into(), vm);
                let _ = mouse_dict.set_item(
                    "is_left_clicking",
                    vm.ctx.new_bool(state.mouse.is_left_clicking).into(),
                    vm,
                );
                let _ = mouse_dict.set_item(
                    "is_right_clicking",
                    vm.ctx.new_bool(state.mouse.is_right_clicking).into(),
                    vm,
                );
                let _ = app_instance.set_attr("mouse", mouse_dict, vm);
                let _ = vm.call_method(app_instance, "on_scroll", (dx, dy));
                try_dispatch_python_on_events(
                    vm,
                    app_instance,
                    state,
                    RoutedPyEvent::Scroll { dx, dy, unit },
                );
            });
        }
    }

    fn on_key_char(&mut self, state: &mut EngineState, ch: char) {
        if let Some(ref app_instance) = self.app_instance {
            self.interpreter.enter(|vm| {
                try_dispatch_python_on_events(vm, app_instance, state, RoutedPyEvent::KeyChar(ch));
            });
        }
    }

    fn on_key_shortcut(&mut self, state: &mut EngineState, shortcut: ShortcutAction) {
        if let Some(ref app_instance) = self.app_instance {
            self.interpreter.enter(|vm| {
                try_dispatch_python_on_events(vm, app_instance, state, RoutedPyEvent::Shortcut(shortcut));
            });
        }
    }

    fn on_screen_size_change(&mut self, state: &mut EngineState, width: u32, height: u32) {
        if let Some(ref app_instance) = self.app_instance {
            // Set the frame buffer context so Python can write to it
            let shape = state.frame.shape();
            let frame_width = shape[1];
            let frame_height = shape[0];
            let buffer = state.frame.buffer_mut();
            crate::python_api::rasterizer::set_frame_buffer_context(buffer, frame_width, frame_height);
            
            self.interpreter.enter(|vm| {
                // Update frame data before calling the handler
                if let Ok(Some(frame_obj)) = vm.get_attribute_opt(app_instance.clone(), "frame") {
                    let _ = crate::python_api::engine::py_bindings::update_py_frame_state(vm, frame_obj, &mut state.frame);
                }
                // Call the Python handler
                let _ = vm.call_method(app_instance, "on_screen_size_change", (width, height));
            });
            
            // Clear the frame buffer context after handler completes
            crate::python_api::rasterizer::clear_frame_buffer_context();
        }
    }
}
