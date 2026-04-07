use rustpython_vm::{PyRef, VirtualMachine, builtins::PyModule};

pub mod activations;
pub mod layers;

pub fn make_nn_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.nn", vm.ctx.new_dict(), None);
    module
        .set_attr("Module", vm.ctx.types.object_type.to_owned(), vm)
        .unwrap();

    // Expose real Python classes so `class Foo(xos.nn.Module)` works.
    // Runtime behavior is intentionally minimal for now.
    let nn_class_code = r#"
def _xos_tensor(data, shape):
    return __import__("xos").tensor(data, shape)

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

def _prod(shape):
    p = 1
    for d in shape:
        p *= int(d)
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
            if hasattr(v, "_is_parameter") and v._is_parameter:
                out.append(v)
            elif hasattr(v, "parameters"):
                out.extend(v.parameters())
        return out

class Parameter:
    def __init__(self, data, shape):
        self.data = _xos_tensor(data, shape)
        self.grad = [0.0 for _ in range(len(data))]
        self._is_parameter = True

    def zero_grad(self):
        for i in range(len(self.grad)):
            self.grad[i] = 0.0

class Conv2d(Module):
    def __init__(self, in_channels, out_channels, kernel_size, stride=1):
        super().__init__()
        self.in_channels = in_channels
        self.out_channels = out_channels
        self.kernel_size = kernel_size
        self.stride = stride

    def forward(self, x):
        # Placeholder until Burn-backed conv op is wired.
        return x

class Linear(Module):
    def __init__(self, in_features, out_features, bias=True):
        super().__init__()
        self.in_features = in_features
        self.out_features = out_features
        self.out_shape, self.out_size = _normalize_out_features_shape(out_features)
        self.in_size = max(1, int(in_features))
        self.bias = bias
        # Small deterministic init; keeps behavior reproducible.
        w = [((i % 17) - 8) * 0.001 for i in range(self.in_size * self.out_size)]
        self.weight = Parameter(w, (self.in_size, self.out_size))
        self.bias_param = Parameter([0.0 for _ in range(self.out_size)], (self.out_size,)) if bias else None
        self._last_input = None

    def forward(self, x):
        src = _tensor_to_flat_list(x)
        if not src:
            src = [0.0]
        if len(src) < self.in_size:
            src = src + [0.0 for _ in range(self.in_size - len(src))]
        elif len(src) > self.in_size:
            src = src[:self.in_size]
        self._last_input = src
        w = self.weight.data._data["_data"]
        out = [0.0 for _ in range(self.out_size)]
        for j in range(self.out_size):
            s = 0.0
            for i in range(self.in_size):
                s += src[i] * w[i * self.out_size + j]
            if self.bias_param is not None:
                s += self.bias_param.data._data["_data"][j]
            out[j] = s
        t = _xos_tensor(out, (1,) + self.out_shape)
        t._creator = self
        return t

    def backward(self, grad_out):
        if self._last_input is None:
            return
        x = self._last_input
        w = self.weight.data._data["_data"]
        for i in range(self.in_size):
            xi = x[i]
            base = i * self.out_size
            for j in range(self.out_size):
                gj = grad_out[j] if j < len(grad_out) else 0.0
                self.weight.grad[base + j] += xi * gj
        if self.bias_param is not None:
            for j in range(self.out_size):
                gj = grad_out[j] if j < len(grad_out) else 0.0
                self.bias_param.grad[j] += gj

class ReLU(Module):
    def __init__(self):
        super().__init__()

    def forward(self, x):
        # Placeholder activation for now.
        return x

class _LossValue:
    def __init__(self, value, backward_fn):
        self._value = float(value)
        self._backward_fn = backward_fn

    def backward(self):
        if self._backward_fn is not None:
            self._backward_fn()

    def item(self):
        return self._value

class MSELoss:
    def __call__(self, pred, target):
        p = _tensor_to_flat_list(pred)
        t = _tensor_to_flat_list(target)
        n = len(p)
        if n == 0:
            return _LossValue(0.0, lambda: None)
        if len(t) < n:
            t = t + [0.0 for _ in range(n - len(t))]
        else:
            t = t[:n]
        diff = [p[i] - t[i] for i in range(n)]
        loss = sum(d * d for d in diff) / float(n)

        def _bw():
            if hasattr(pred, "_creator"):
                grad = [(2.0 / float(n)) * d for d in diff]
                pred._creator.backward(grad)

        return _LossValue(loss, _bw)

class Adam:
    def __init__(self, params, lr=0.001, betas=(0.9, 0.999), eps=1e-8):
        self.params = list(params)
        self.lr = float(lr)
        self.beta1 = float(betas[0])
        self.beta2 = float(betas[1])
        self.eps = float(eps)
        self.t = 0
        self.m = [[0.0 for _ in p.grad] for p in self.params]
        self.v = [[0.0 for _ in p.grad] for p in self.params]

    def zero_grad(self):
        for p in self.params:
            p.zero_grad()

    def step(self):
        self.t += 1
        b1t = 1.0 - (self.beta1 ** self.t)
        b2t = 1.0 - (self.beta2 ** self.t)
        for pi, p in enumerate(self.params):
            pdata = p.data._data["_data"]
            g = p.grad
            m = self.m[pi]
            v = self.v[pi]
            for i in range(len(pdata)):
                gi = g[i]
                m[i] = self.beta1 * m[i] + (1.0 - self.beta1) * gi
                v[i] = self.beta2 * v[i] + (1.0 - self.beta2) * gi * gi
                mh = m[i] / b1t
                vh = v[i] / b2t
                pdata[i] -= self.lr * mh / ((vh ** 0.5) + self.eps)
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
        // Fallbacks to keep module shape available if class code fails.
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
