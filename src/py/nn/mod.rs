use rustpython_vm::{PyRef, VirtualMachine, builtins::PyModule};

pub mod activations;
pub mod layers;

pub fn make_nn_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.nn", vm.ctx.new_dict(), None);
    module
        .set_attr("Module", vm.ctx.types.object_type.to_owned(), vm)
        .unwrap();
    module
        .set_attr(
            "_conv2d_forward_native",
            vm.new_function("_conv2d_forward_native", layers::conv2d_forward),
            vm,
        )
        .unwrap();
    module
        .set_attr(
            "_relu_forward_native",
            vm.new_function("_relu_forward_native", layers::relu_forward),
            vm,
        )
        .unwrap();

    let nn_class_code = r#"
def _x():
    return __import__("xos")

def _normalize_out_features_shape(out_features):
    if isinstance(out_features, tuple):
        dims = tuple(max(1, int(d)) for d in out_features)
        return dims, _shape_product(dims)
    if isinstance(out_features, list):
        dims = tuple(max(1, int(d)) for d in out_features)
        return dims, _shape_product(dims)
    out = max(1, int(out_features))
    return (out,), out

def _shape_product(dims):
    p = 1
    for d in dims:
        p *= d
    return p

def _tensor_to_flat_list(t):
    out = []

    def _flatten_any(obj):
        if isinstance(obj, (list, tuple)):
            for it in obj:
                _flatten_any(it)
            return
        try:
            out.append(float(obj))
        except Exception:
            pass

    if hasattr(t, "_data"):
        data_obj = t._data
        if isinstance(data_obj, dict):
            raw = data_obj.get("_data", None)
            if raw is None:
                raw = data_obj.get("data", None)
            if raw is not None:
                _flatten_any(raw)
                if out:
                    return out
            viewport_id = data_obj.get("_xos_viewport_id", None)
            if viewport_id is not None:
                try:
                    raw = _x().frame._standalone_tensor_data(int(viewport_id))
                    _flatten_any(raw)
                    if out:
                        return out
                except Exception:
                    pass
        else:
            _flatten_any(data_obj)
            if out:
                return out

    if hasattr(t, "get"):
        raw = t.get("_data", None)
        if raw is None:
            raw = t.get("data", None)
        if raw is not None:
            _flatten_any(raw)
            if out:
                return out

    if hasattr(t, "list"):
        try:
            _flatten_any(t.list())
            if out:
                return out
        except Exception:
            pass

    return out

class Module:
    def __init__(self):
        pass

    def forward(self, x):
        return x

    def __call__(self, x):
        return self.forward(x)

    def parameters(self):
        out = []
        for _k, v in self.__dict__.items():
            if isinstance(v, Linear):
                out.append(v)
            elif isinstance(v, Parameter):
                out.append(v)
            elif hasattr(v, "parameters"):
                out.extend(v.parameters())
        return out

class Parameter:
    """Lightweight parameter wrapper used by model inspectors."""
    def __init__(self, name, shape, dtype, values=None, stats=None):
        self.name = str(name)
        self.shape = tuple(int(d) for d in shape)
        self.dtype = str(dtype)
        self.values = list(values) if values is not None else []
        self.stats = dict(stats) if stats is not None else {}

    def mean(self):
        if "mean" in self.stats:
            return float(self.stats["mean"])
        if not self.values:
            return 0.0
        return float(sum(float(v) for v in self.values) / len(self.values))

    def min(self):
        if "min" in self.stats:
            return float(self.stats["min"])
        if not self.values:
            return 0.0
        return float(min(float(v) for v in self.values))

    def max(self):
        if "max" in self.stats:
            return float(self.stats["max"])
        if not self.values:
            return 0.0
        return float(max(float(v) for v in self.values))

    def std(self):
        if not self.values:
            return 0.0
        m = self.mean()
        var = sum((float(v) - m) ** 2 for v in self.values) / float(len(self.values))
        return var ** 0.5

class Conv2d(Module):
    def __init__(self, in_channels, out_channels, kernel_size, stride=1, averaged=True):
        super().__init__()
        self.in_channels = in_channels
        self.out_channels = out_channels
        self.kernel_size = kernel_size
        self.stride = stride
        self.averaged = bool(averaged)
        xos = _x()
        self._burn_conv2d_id = int(
            xos._burn.conv2d_register(
                in_channels=max(1, int(in_channels)),
                out_channels=max(1, int(out_channels)),
                kernel_size=kernel_size,
                stride=stride,
                averaged=self.averaged,
            )
        )

    @property
    def weights(self):
        return _x()._burn.conv2d_weights(self._burn_conv2d_id)

    def forward(self, x):
        return _x()._burn.conv2d_forward_tensor(self._burn_conv2d_id, x)

class Linear(Module):
    """Burn-backed linear layer (Autodiff + ndarray)."""

    def __init__(self, in_features, out_features, bias=True):
        super().__init__()
        self.in_features = in_features
        self.out_features = out_features
        self.out_shape, self.out_size = _normalize_out_features_shape(out_features)
        if in_features is None:
            self.in_size = None
        else:
            self.in_size = max(1, int(in_features))
        self.bias = bias
        self._burn_linear_id = None
        if self.in_size is not None:
            xos = _x()
            self._burn_linear_id = int(
                xos._burn.linear_register(
                    in_features=self.in_size,
                    out_features=self.out_size,
                    bias=bias,
                )
            )

    def _ensure_initialized(self, input_size):
        input_size = max(1, int(input_size))
        if self._burn_linear_id is None:
            self.in_size = input_size
            self._burn_linear_id = int(
                _x()._burn.linear_register(
                    in_features=self.in_size,
                    out_features=self.out_size,
                    bias=self.bias,
                )
            )
            return
        if self.in_size != input_size:
            raise ValueError(
                f"Linear input size mismatch: got {input_size}, expected {self.in_size}. "
                "Set in_features correctly, or initialize with in_features=None for lazy init."
            )

    @property
    def weights(self):
        if self._burn_linear_id is None:
            raise ValueError(
                "Linear weights are not initialized yet. Run one forward pass first when using in_features=None."
            )
        return _x()._burn.linear_weights(self._burn_linear_id)

    def forward(self, x):
        flat = _tensor_to_flat_list(x)
        self._ensure_initialized(len(flat))
        out = _x()._burn.linear_forward(self._burn_linear_id, flat)
        if len(self.out_shape) > 1:
            out = out.reshape((1,) + tuple(self.out_shape))
        try:
            out._burn_linear_id = self._burn_linear_id
        except Exception:
            pass
        return out

class ReLU(Module):
    def __init__(self):
        super().__init__()

    def forward(self, x):
        return _x().nn._relu_forward_native(self, x)

class _BurnLoss:
    def backward(self):
        _x()._burn.loss_backward()

    def item(self):
        return float(_x()._burn.loss_item())

class MSELoss:
    """Mean squared error via Burn autograd (pred must be Burn Linear output)."""

    def __call__(self, pred, target):
        lid = getattr(pred, "_burn_linear_id", None)
        if lid is None:
            raise TypeError("MSELoss expects pred from a Burn-backed xos.nn.Linear forward()")
        tflat = _tensor_to_flat_list(target)
        _x()._burn.mse_loss(int(lid), tflat)
        return _BurnLoss()

class Adam:
    """Burn Adam optimizer (OptimizerAdaptor + burn::optim::Adam)."""

    def __init__(self, params, lr=0.001, betas=(0.9, 0.999), eps=1e-8):
        self.lr = float(lr)
        self.betas = betas
        self.eps = float(eps)
        self._params = list(params)
        self._optim_id = None
        self._try_init()

    def _try_init(self):
        if self._optim_id is not None:
            return True
        linear_id = None
        for p in self._params:
            lid = getattr(p, "_burn_linear_id", None)
            if lid is not None:
                linear_id = int(lid)
                break
        if linear_id is None:
            return False
        self._optim_id = int(
            _x()._burn.adam_init(
                linear_id=linear_id,
                lr=self.lr,
                beta1=float(self.betas[0]),
                beta2=float(self.betas[1]),
                eps=self.eps,
            )
        )
        return True

    def zero_grad(self):
        if self._optim_id is None:
            self._try_init()
        if self._optim_id is None:
            return
        _x()._burn.adam_zero_grad()

    def step(self):
        if self._optim_id is None and not self._try_init():
            raise ValueError(
                "Adam not initialized yet: run one forward pass first so lazy Linear layers can infer input size."
            )
        _x()._burn.adam_step(self._optim_id, self.lr)
"#;
    let scope = vm.new_scope_with_builtins();
    match vm.run_code_string(scope.clone(), nn_class_code, "<xos_nn>".to_string()) {
        Ok(_) => {
            if let Ok(module_cls) = scope.globals.get_item("Module", vm) {
                module.set_attr("Module", module_cls, vm).unwrap();
            }
            if let Ok(conv2d_cls) = scope.globals.get_item("Conv2d", vm) {
                module.set_attr("Conv2d", conv2d_cls, vm).unwrap();
            }
            if let Ok(linear_cls) = scope.globals.get_item("Linear", vm) {
                module.set_attr("Linear", linear_cls, vm).unwrap();
            }
            if let Ok(relu_cls) = scope.globals.get_item("ReLU", vm) {
                module.set_attr("ReLU", relu_cls, vm).unwrap();
            }
            if let Ok(param_cls) = scope.globals.get_item("Parameter", vm) {
                module.set_attr("Parameter", param_cls, vm).unwrap();
            }
            let losses = vm.new_module("xos.nn.losses", vm.ctx.new_dict(), None);
            if let Ok(mse_cls) = scope.globals.get_item("MSELoss", vm) {
                losses.set_attr("MSELoss", mse_cls, vm).unwrap();
            }
            module.set_attr("losses", losses, vm).unwrap();
            let optimizers = vm.new_module("xos.nn.optimizers", vm.ctx.new_dict(), None);
            if let Ok(adam_cls) = scope.globals.get_item("Adam", vm) {
                optimizers.set_attr("Adam", adam_cls, vm).unwrap();
            }
            module.set_attr("optimizers", optimizers, vm).unwrap();
        }
        Err(err) => {
            eprintln!("Failed to initialize xos.nn classes: {:?}", err);
            module
                .set_attr("Conv2d", vm.new_function("Conv2d", layers::conv2d_new), vm)
                .unwrap();
            module
                .set_attr("Linear", vm.new_function("Linear", layers::linear_new), vm)
                .unwrap();
            module
                .set_attr("ReLU", vm.new_function("ReLU", activations::relu_new), vm)
                .unwrap();
        }
    }
    module
}
