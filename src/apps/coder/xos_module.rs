use crate::engine::EngineState;

/// Frame buffer wrapper that allows Python to access and modify the frame buffer
/// This is passed to Python code via a callback mechanism
pub struct FrameBuffer {
    pub state_ptr: *mut EngineState,
}

unsafe impl Send for FrameBuffer {}
unsafe impl Sync for FrameBuffer {}

impl FrameBuffer {
    pub fn new(state: &mut EngineState) -> Self {
        Self {
            state_ptr: state as *mut EngineState,
        }
    }

    pub fn get_buffer(&self) -> &mut [u8] {
        unsafe {
            (*self.state_ptr).frame_buffer_mut()
        }
    }

    pub fn get_shape(&self) -> Vec<usize> {
        unsafe {
            (*self.state_ptr).frame.shape().to_vec()
        }
    }
}

/// Generate Python code to create the xos module with frame buffer access
pub fn generate_xos_module_code(width: usize, height: usize) -> String {
    format!(r#"
import sys
import types

class FrameBuffer:
    def __init__(self, width, height):
        self.width = width
        self.height = height
        # Frame buffer as flat array: [height, width, 4] RGBA
        # We'll use a list for now, but this will be backed by Rust memory
        self._data = [0] * (width * height * 4)
    
    def __getitem__(self, key):
        """Access pixel at [y, x] or slice"""
        if isinstance(key, tuple) and len(key) == 2:
            y, x = key
            if 0 <= y < self.height and 0 <= x < self.width:
                idx = (y * self.width + x) * 4
                return self._data[idx:idx+4]
        elif isinstance(key, int):
            # Return row
            if 0 <= key < self.height:
                start = key * self.width * 4
                end = start + self.width * 4
                return self._data[start:end]
        raise IndexError("Invalid index")
    
    def __setitem__(self, key, value):
        """Set pixel at [y, x] or slice"""
        if isinstance(key, tuple) and len(key) == 2:
            y, x = key
            if 0 <= y < self.height and 0 <= x < self.width:
                idx = (y * self.width + x) * 4
                if isinstance(value, (list, tuple)) and len(value) >= 4:
                    self._data[idx:idx+4] = value[:4]
                elif isinstance(value, int):
                    # Set all channels to same value
                    self._data[idx:idx+4] = [value, value, value, 255]
        elif isinstance(key, int):
            # Set entire row
            if 0 <= key < self.height:
                start = key * self.width * 4
                end = start + self.width * 4
                if isinstance(value, (list, tuple)):
                    self._data[start:end] = value[:self.width*4]
        else:
            raise IndexError("Invalid index")
    
    def shape(self):
        return (self.height, self.width, 4)

class Array:
    """Wrapper around xos Array type"""
    def __init__(self, data, shape):
        self.data = list(data)
        self.shape = tuple(shape)
    
    def __getitem__(self, key):
        return self.data[key]
    
    def __setitem__(self, key, value):
        self.data[key] = value
    
    def shape(self):
        return self.shape

# Create xos module
xos_module = types.ModuleType('xos')
xos_module.frame = FrameBuffer({}, {})
xos_module.array = Array

# Add to sys.modules
sys.modules['xos'] = xos_module
"#, width, height)
}

