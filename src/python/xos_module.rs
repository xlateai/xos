#[cfg(feature = "python")]
use rustpython_vm::{PyResult, VirtualMachine, builtins::PyModule, PyRef, function::FuncArgs};

/// The xos.hello() function
fn hello(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    println!("hello from xos module");
    Ok(vm.ctx.none())
}

/// xos.get_frame_buffer() - returns the frame buffer dimensions and a placeholder
/// In the future, this will return actual frame buffer access
fn get_frame_buffer(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    // For now, return a dict with width, height, and placeholder buffer
    let dict = vm.ctx.new_dict();
    dict.set_item("width", vm.ctx.new_int(800).into(), vm)?;
    dict.set_item("height", vm.ctx.new_int(600).into(), vm)?;
    dict.set_item("buffer", vm.ctx.new_list(vec![]).into(), vm)?;
    Ok(dict.into())
}

/// xos.get_mouse() - returns mouse position
fn get_mouse(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let dict = vm.ctx.new_dict();
    dict.set_item("x", vm.ctx.new_float(0.0).into(), vm)?;
    dict.set_item("y", vm.ctx.new_float(0.0).into(), vm)?;
    dict.set_item("down", vm.ctx.new_bool(false).into(), vm)?;
    Ok(dict.into())
}

/// xos.print() - print to xos console
fn xos_print(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let msg: String = args.bind(vm)?;
    println!("[xos] {}", msg);
    Ok(vm.ctx.none())
}

/// Create the xos module with Application base class
pub fn make_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos", vm.ctx.new_dict(), None);
    
    // Add functions to the module
    module.set_attr("hello", vm.new_function("hello", hello), vm).unwrap();
    module.set_attr("get_frame_buffer", vm.new_function("get_frame_buffer", get_frame_buffer), vm).unwrap();
    module.set_attr("get_mouse", vm.new_function("get_mouse", get_mouse), vm).unwrap();
    module.set_attr("print", vm.new_function("print", xos_print), vm).unwrap();
    
    // Add the random submodule
    let random_module = crate::python::random::random::make_random_module(vm);
    module.set_attr("random", random_module, vm).unwrap();
    
    // Define the Application base class in Python
    let application_class_code = r#"
class Application:
    """Base class for xos applications. Extend this class and implement setup() and tick()."""
    
    def setup(self):
        """Called once when the application starts. Override this method."""
        raise NotImplementedError("Subclasses must implement setup()")
    
    def tick(self):
        """Called every frame. Override this method."""
        raise NotImplementedError("Subclasses must implement tick()")
    
    def on_mouse_down(self, x, y):
        """Called when mouse is clicked. Override this method (optional)."""
        pass
    
    def on_mouse_up(self, x, y):
        """Called when mouse is released. Override this method (optional)."""
        pass
    
    def on_mouse_move(self, x, y):
        """Called when mouse moves. Override this method (optional)."""
        pass
    
    def run(self):
        """Run the application. Calls setup() once, then tick() in a loop."""
        print("[xos] Starting application...")
        
        # Call setup
        self.setup()
        
        # Simple game loop (for now, just run a few ticks as demo)
        # TODO: This will be replaced with actual engine integration
        print("[xos] Running game loop (demo mode - 10 ticks)...")
        for i in range(10):
            self.tick()
        
        print("[xos] Application finished (demo mode)")
"#;
    
    // Execute the Application class definition
    let scope = vm.new_scope_with_builtins();
    if let Err(e) = vm.run_code_string(scope.clone(), application_class_code, "<xos_module>".to_string()) {
        eprintln!("Failed to create Application class: {:?}", e);
    }
    
    // Get the Application class from the scope and add it to the module
    if let Ok(app_class) = scope.globals.get_item("Application", vm) {
        module.set_attr("Application", app_class, vm).unwrap();
    }
    
    module
}

