import Foundation

// C-compatible function declarations for Rust functions
@_silgen_name("xos_engine_init")
func xos_engine_init(_ app_name: UnsafePointer<CChar>?, _ width: UInt32, _ height: UInt32) -> UnsafeMutablePointer<CChar>?

@_silgen_name("xos_engine_init_free")
func xos_engine_init_free(_ ptr: UnsafeMutablePointer<CChar>?)

@_silgen_name("xos_engine_tick")
func xos_engine_tick() -> Int32

@_silgen_name("xos_engine_get_frame_buffer")
func xos_engine_get_frame_buffer() -> UnsafePointer<UInt8>?

@_silgen_name("xos_engine_get_frame_buffer_size")
func xos_engine_get_frame_buffer_size() -> Int

@_silgen_name("xos_engine_get_frame_size")
func xos_engine_get_frame_size(_ width: UnsafeMutablePointer<UInt32>?, _ height: UnsafeMutablePointer<UInt32>?) -> Int32

@_silgen_name("xos_engine_update_mouse")
func xos_engine_update_mouse(_ x: Float, _ y: Float) -> Int32

@_silgen_name("xos_engine_mouse_down")
func xos_engine_mouse_down() -> Int32

@_silgen_name("xos_engine_mouse_up")
func xos_engine_mouse_up() -> Int32

@_silgen_name("xos_engine_resize")
func xos_engine_resize(_ width: UInt32, _ height: UInt32) -> Int32

/// Normalized safe rect (`x2`/`y2` = right/bottom), same convention as Rust `SafeRegionBoundingRectangle`.
@_silgen_name("xos_engine_set_safe_region")
func xos_engine_set_safe_region(_ x1: Float, _ y1: Float, _ x2: Float, _ y2: Float) -> Int32

@_silgen_name("xos_engine_cleanup")
func xos_engine_cleanup()

@_silgen_name("xos_engine_toggle_f3_menu")
func xos_engine_toggle_f3_menu() -> Int32

@_silgen_name("xos_set_log_callback")
func xos_set_log_callback(_ callback: @convention(c) (UnsafePointer<CChar>?) -> Void)

@_silgen_name("xos_list_applications_count")
func xos_list_applications_count() -> Int

@_silgen_name("xos_list_applications_get_name")
func xos_list_applications_get_name(_ index: Int) -> UnsafeMutablePointer<CChar>?

@_silgen_name("xos_list_applications_free_name")
func xos_list_applications_free_name(_ ptr: UnsafeMutablePointer<CChar>?)

/// Swift wrapper for initializing the engine
public func xosEngineInit(appName: String, width: UInt32, height: UInt32) throws {
    let appNameCString = appName.cString(using: .utf8)
    guard let appNamePtr = appNameCString else {
        throw XosError.invalidInput("Invalid app name encoding")
    }
    
    guard let errorPtr = xos_engine_init(appNamePtr, width, height) else {
        return // Success
    }
    
    defer {
        xos_engine_init_free(errorPtr)
    }
    
    let errorString = String(cString: errorPtr)
    throw XosError.initializationFailed(errorString)
}

/// Swift wrapper for ticking the engine
@discardableResult
public func xosEngineTick() -> Bool {
    return xos_engine_tick() == 0
}

/// Swift wrapper for listing all available applications
public func xosListApplications() -> [String] {
    let count = xos_list_applications_count()
    var apps: [String] = []
    
    for i in 0..<count {
        if let namePtr = xos_list_applications_get_name(i) {
            let appName = String(cString: namePtr)
            apps.append(appName)
            xos_list_applications_free_name(namePtr)
        }
    }
    
    return apps
}

/// Swift wrapper for getting frame buffer
public func xosEngineGetFrameBuffer() -> UnsafePointer<UInt8>? {
    return xos_engine_get_frame_buffer()
}

/// Swift wrapper for getting frame buffer size
public func xosEngineGetFrameBufferSize() -> Int {
    return xos_engine_get_frame_buffer_size()
}

/// Swift wrapper for getting frame dimensions
public func xosEngineGetFrameSize() -> (width: UInt32, height: UInt32)? {
    var width: UInt32 = 0
    var height: UInt32 = 0
    
    guard xos_engine_get_frame_size(&width, &height) == 0 else {
        return nil
    }
    
    return (width: width, height: height)
}

/// Swift wrapper for updating mouse position
@discardableResult
public func xosEngineUpdateMouse(x: Float, y: Float) -> Bool {
    return xos_engine_update_mouse(x, y) == 0
}

/// Swift wrapper for mouse down event
@discardableResult
public func xosEngineMouseDown() -> Bool {
    return xos_engine_mouse_down() == 0
}

/// Swift wrapper for mouse up event
@discardableResult
public func xosEngineMouseUp() -> Bool {
    return xos_engine_mouse_up() == 0
}

/// Swift wrapper for resizing the frame
@discardableResult
public func xosEngineResize(width: UInt32, height: UInt32) -> Bool {
    return xos_engine_resize(width, height) == 0
}

/// Pushes `UIView.safeAreaInsets` into the engine so layout / Python `safe_region` match the device.
@discardableResult
public func xosEngineSetSafeRegion(x1: Float, y1: Float, x2: Float, y2: Float) -> Bool {
    return xos_engine_set_safe_region(x1, y1, x2, y2) == 0
}

/// Swift wrapper for cleanup
public func xosEngineCleanup() {
    xos_engine_cleanup()
}

/// XOS Engine Errors
public enum XosError: Error {
    case invalidInput(String)
    case initializationFailed(String)
    case engineNotInitialized
}

