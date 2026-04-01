use rustpython_vm::{Interpreter, PyObjectRef, AsObject, VirtualMachine, builtins::PyBaseExceptionRef};
use crate::engine::{Application, EngineState};

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

pub const APPLICATION_CLASS_CODE: &str = r#"
class _ArrayResult:
    """Wrapper for list results that provides nice string representation"""
    def __init__(self, data, shape=None):
        self._data = data
        self._shape = shape
    
    def __iter__(self):
        return iter(self._data)
    
    def __len__(self):
        return len(self._data)
    
    def __getitem__(self, idx):
        return self._data[idx]
    
    def __str__(self):
        if not self._data:
            return "xos.Array(empty)"
        min_val = min(self._data)
        max_val = max(self._data)
        mean_val = sum(self._data) / len(self._data)
        shape_str = f"shape={self._shape}" if self._shape else f"len={len(self._data)}"
        return f"xos.Array({shape_str}, min={min_val:.1f}, mean={mean_val:.1f}, max={max_val:.1f})"
    
    def __repr__(self):
        return self.__str__()

class _ArrayWrapper:
    """Wrapper for array dict that supports slice assignment"""
    def __init__(self, data):
        self._data = data
    
    def __getitem__(self, key):
        if isinstance(key, slice) and key == slice(None, None, None):
            # Return the underlying data dict for full slice access
            return self._data
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
        if isinstance(key, _ArrayWrapper):
            return self._gather_rows(key)
        return self._data[key]

    def _wrap_vals(self, shape, values):
        return _ArrayWrapper({
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
            # Call Rust function to fill buffer (handles lists and _ArrayResult)
            import xos
            xos.rasterizer._fill_buffer(self._data, value)
        else:
            self._data[key] = value

    def _wrap_like_self(self, values):
        return _ArrayWrapper({
            "shape": self.shape,
            "dtype": self.dtype,
            "device": self._data.get("device", "cpu"),
            "_data": values,
        })

    def _binary_op(self, other, op):
        left = self._data.get("_data", [])
        lshape = tuple(self.shape)
        if isinstance(other, _ArrayWrapper):
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
        if isinstance(other, _ArrayWrapper):
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
        if isinstance(other, _ArrayWrapper):
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
        if isinstance(other, _ArrayWrapper):
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
    
    def size(self):
        """Return the total number of elements (product of all dimensions)"""
        shape = self.shape
        if not shape:
            return 0
        size = 1
        for dim in shape:
            size *= dim
        return size
    
    def __str__(self):
        # For regular arrays (not frame arrays), compute statistics
        if '_data' in self._data:
            data_list = self._data['_data']
            if data_list and len(data_list) > 0:
                min_val = min(data_list)
                max_val = max(data_list)
                mean_val = sum(data_list) / len(data_list)
                return f"xos.Array(shape={self.shape}, dtype={self.dtype}, min={min_val:.3f}, max={max_val:.3f}, mean={mean_val:.3f})"
            return f"xos.Array(shape={self.shape}, dtype={self.dtype}, empty)"
        # For frame arrays (no _data field)
        return f"xos.Array(shape={self.shape}, dtype=u8)"
    
    def __repr__(self):
        return self.__str__()

class _FrameWrapper:
    """Wrapper to make frame dict behave like an object with methods"""
    def __init__(self, data):
        self._data = data
        self._tensor_wrapper = _ArrayWrapper(data.get('tensor', {}))
    
    @property
    def tensor(self):
        """Get the tensor wrapper with slice assignment support (CPU RGBA frame)."""
        return self._tensor_wrapper
    
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

        xos.rasterizer.fill(self, rgba)

class Application:
    """Base class for xos applications. Extend this class and implement __init__() and tick()."""
    
    def __init__(self, headless=None):
        self._xos_initialized = True
        self._xos_engine_bound = False
        self.frame = None  # Will be set by the engine
        self.mouse = None  # Will be set by the engine
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
            ws = xos.frame._standalone_window_size()
            if ws is not None:
                w, h = ws
                self._xos_standalone_width = int(max(1, w))
                self._xos_standalone_height = int(max(1, h))
            sp = xos.frame._standalone_ui_scale()
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
            int(getattr(self, "_xos_standalone_width", 800)),
            int(getattr(self, "_xos_standalone_height", 600)),
        )
        self.frame = _FrameWrapper(frame_dict)
        self.mouse = {"x": 0.0, "y": 0.0, "is_left_clicking": False}
        self.pre_tick()

    def _xos_post_tick(self):
        import xos
        try:
            self.post_tick()
        finally:
            if not getattr(self, "_xos_engine_bound", False):
                if not bool(getattr(self, "headless", False)):
                    xos.frame._present_standalone()
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
                
                // Wrap it in _FrameWrapper
                if let Ok(wrapper_class) = vm.builtins.get_attr("_FrameWrapper", vm) {
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
                    let _ = app_instance.set_attr("mouse", mouse_dict, vm);

                    // Expose timestep and FPS directly to Python app.
                    let timestep = state.delta_time_seconds.max(1e-5) as f64;
                    let _ = app_instance.set_attr("dt", vm.ctx.new_float(timestep), vm);
                    let _ = app_instance.set_attr("fps", vm.ctx.new_float(1.0 / timestep), vm);

                    // Tick counter: value during tick() is N ticks completed so far (0 on first tick).
                    let _ = app_instance.set_attr("t", vm.ctx.new_int(tick_index as usize), vm);
                    let _ = app_instance.set_attr("xos_scale", vm.ctx.new_float(state.ui_scale_percent as f64 / 100.0), vm);
                    
                    // Call tick
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
        if let Some(ref app_instance) = self.app_instance {
            self.interpreter.enter(|vm| {
                let x = state.mouse.x;
                let y = state.mouse.y;
                let _ = vm.call_method(app_instance, "on_mouse_down", (x, y));
            });
        }
    }

    fn on_mouse_up(&mut self, state: &mut EngineState) {
        if let Some(ref app_instance) = self.app_instance {
            self.interpreter.enter(|vm| {
                let x = state.mouse.x;
                let y = state.mouse.y;
                let _ = vm.call_method(app_instance, "on_mouse_up", (x, y));
            });
        }
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        if let Some(ref app_instance) = self.app_instance {
            self.interpreter.enter(|vm| {
                let x = state.mouse.x;
                let y = state.mouse.y;
                let _ = vm.call_method(app_instance, "on_mouse_move", (x, y));
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
