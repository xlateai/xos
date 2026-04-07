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
    if hasattr(t, "_data"):
        data = t._data
        if hasattr(data, "get"):
            data = data.get("_data", [])
        for v in data:
            try:
                out.append(float(v))
            except Exception:
                pass
        return out
    if hasattr(t, "get"):
        data = t.get("_data", [])
        for v in data:
            try:
                out.append(float(v))
            except Exception:
                pass
        return out
    return []

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
            if isinstance(v, Linear) and getattr(v, "_burn_linear_id", None) is not None:
                out.append(v)
            elif hasattr(v, "parameters"):
                out.extend(v.parameters())
        return out

class Conv2d(Module):
    def __init__(self, in_channels, out_channels, kernel_size, stride=1):
        super().__init__()
        self.in_channels = in_channels
        self.out_channels = out_channels
        self.kernel_size = kernel_size
        self.stride = stride
        xos = _x()
        self._burn_conv2d_id = int(
            xos._burn.conv2d_register(
                in_channels=max(1, int(in_channels)),
                out_channels=max(1, int(out_channels)),
                kernel_size=kernel_size,
                stride=stride,
            )
        )

    def forward(self, x):
        shape = getattr(x, "shape", None)
        if shape is None:
            shape = (600, 800, max(1, int(self.in_channels)))
        flat = _tensor_to_flat_list(x)
        return _x()._burn.conv2d_forward(self._burn_conv2d_id, flat, tuple(shape))

class Linear(Module):
    """Burn-backed linear layer (Autodiff + ndarray)."""

    def __init__(self, in_features, out_features, bias=True):
        super().__init__()
        self.in_features = in_features
        self.out_features = out_features
        self.out_shape, self.out_size = _normalize_out_features_shape(out_features)
        self.in_size = max(1, int(in_features))
        self.bias = bias
        xos = _x()
        self._burn_linear_id = int(
            xos._burn.linear_register(
                in_features=self.in_size,
                out_features=self.out_size,
                bias=bias,
            )
        )

    def forward(self, x):
        flat = _tensor_to_flat_list(x)
        out = _x()._burn.linear_forward(self._burn_linear_id, flat)
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
        linear_id = None
        for p in params:
            lid = getattr(p, "_burn_linear_id", None)
            if lid is not None:
                linear_id = int(lid)
                break
        if linear_id is None:
            raise ValueError(
                "Adam requires a Burn Linear in parameters() (e.g. include self.linear1 from Agent)"
            )
        self._optim_id = int(
            _x()._burn.adam_init(
                linear_id=linear_id,
                lr=self.lr,
                beta1=float(betas[0]),
                beta2=float(betas[1]),
                eps=self.eps,
            )
        )

    def zero_grad(self):
        _x()._burn.adam_zero_grad()

    def step(self):
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
