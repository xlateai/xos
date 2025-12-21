#[cfg(feature = "python")]
pub const APPLICATION_CLASS_CODE: &str = r#"
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

