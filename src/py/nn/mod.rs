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
        self.bias = bias

    def forward(self, x):
        # Deterministic placeholder projection to fixed feature width.
        # Repeats/truncates source values and reshapes to configured out_features.
        src = _tensor_to_flat_list(x)
        if not src:
            src = [0.0]
        out = [src[i % len(src)] for i in range(self.out_size)]
        return _xos_tensor(out, (1,) + self.out_shape)

class ReLU(Module):
    def __init__(self):
        super().__init__()

    def forward(self, x):
        # Placeholder activation for now.
        return x
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
