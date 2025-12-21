import Foundation

// Conditionally import CoreMotion - if it fails, we'll handle gracefully
#if canImport(CoreMotion)
import CoreMotion
#else
// CoreMotion not available - define stub types
typealias CMMotionManager = Any
#endif

// Magnetometer callback type for Rust
typealias MagnetometerCallback = @convention(c) (Double, Double, Double, UnsafeMutableRawPointer?) -> Void

// Magnetometer listener manager
final class MagnetometerListenerManager {
    static let shared = MagnetometerListenerManager()
    
    private var listeners: [UInt32: MagnetometerListener] = [:]
    private var nextListenerId: UInt32 = 0
    private let lock = NSLock()
    
    private init() {}
    
    func createListener() -> UInt32? {
        lock.lock()
        defer { lock.unlock() }
        
        let listenerId = nextListenerId
        nextListenerId += 1
        
        // Wrap in do-catch to prevent any exceptions from propagating
        do {
            print("[MagnetometerListenerManager] Creating listener ID=\(listenerId)")
            let listener = try MagnetometerListener(listenerId: listenerId)
            listeners[listenerId] = listener
            print("[MagnetometerListenerManager] Successfully created listener ID=\(listenerId)")
            return listenerId
        } catch {
            print("[MagnetometerListenerManager] Failed to create listener: \(error.localizedDescription)")
            return nil
        }
    }
    
    func getListener(_ listenerId: UInt32) -> MagnetometerListener? {
        lock.lock()
        defer { lock.unlock() }
        return listeners[listenerId]
    }
    
    func destroyListener(_ listenerId: UInt32) {
        lock.lock()
        defer { lock.unlock() }
        
        if let listener = listeners[listenerId] {
            listener.stop()
            listeners.removeValue(forKey: listenerId)
            print("[MagnetometerListenerManager] Destroyed listener ID=\(listenerId)")
        }
    }
}

// Magnetometer listener
final class MagnetometerListener {
    private let listenerId: UInt32
    #if canImport(CoreMotion)
    private let motionManager: CMMotionManager
    #else
    private let motionManager: Any? = nil
    #endif
    private var callback: MagnetometerCallback?
    private var callbackUserData: UnsafeMutableRawPointer?
    private var isActive: Bool = false
    
    init(listenerId: UInt32) throws {
        self.listenerId = listenerId
        
        #if canImport(CoreMotion)
        // Create motion manager - CMMotionManager can be created on any thread
        // but operations should be performed on main thread
        let manager = CMMotionManager()
        self.motionManager = manager
        
        // Check if magnetometer is available
        guard manager.isMagnetometerAvailable else {
            throw NSError(
                domain: "MagnetometerListener",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "Magnetometer is not available on this device"]
            )
        }
        
        // Set update interval (100ms = 0.1 seconds)
        manager.magnetometerUpdateInterval = 0.01
        
        print("[MagnetometerListener] Initialized listener ID=\(listenerId)")
        #else
        throw NSError(
            domain: "MagnetometerListener",
            code: 1,
            userInfo: [NSLocalizedDescriptionKey: "CoreMotion framework not available"]
        )
        #endif
    }
    
    func setCallback(_ callback: @escaping MagnetometerCallback, userData: UnsafeMutableRawPointer?) {
        self.callback = callback
        self.callbackUserData = userData
    }
    
    func start() throws {
        #if canImport(CoreMotion)
        guard !isActive else {
            print("[MagnetometerListener] Already started")
            return
        }
        
        guard let manager = motionManager as? CMMotionManager else {
            throw NSError(
                domain: "MagnetometerListener",
                code: 2,
                userInfo: [NSLocalizedDescriptionKey: "Motion manager not available"]
            )
        }
        
        guard manager.isMagnetometerAvailable else {
            throw NSError(
                domain: "MagnetometerListener",
                code: 2,
                userInfo: [NSLocalizedDescriptionKey: "Magnetometer is not available"]
            )
        }
        
        // Start magnetometer updates
        // Note: startMagnetometerUpdates doesn't throw, but we wrap the callback in a safe handler
        manager.startMagnetometerUpdates(to: .main) { [weak self] (data, error) in
            // Capture callback and userData early to avoid accessing self after deallocation check
            guard let self = self else {
                // Listener was deallocated, ignore callback
                return
            }
            
            // Check if still active before processing
            guard self.isActive else {
                // Listener was stopped, ignore callback
                return
            }
            
            // Capture callback and userData while self is still valid
            guard let callback = self.callback else {
                // Callback not set, ignore
                return
            }
            
            let userData = self.callbackUserData
            
            if let error = error {
                print("[MagnetometerListener] Error: \(error.localizedDescription)")
                return
            }
            
            guard let magnetometerData = data else {
                return
            }
            
            // CMMagneticField values are already in microtesla (μT), no conversion needed
            let x = magnetometerData.magneticField.x
            let y = magnetometerData.magneticField.y
            let z = magnetometerData.magneticField.z
            
            // Safely call the Rust callback with captured values
            // Wrap in autoreleasepool to prevent memory issues
            autoreleasepool {
                // Double-check that callback and userData are still valid
                // (They might have been cleared during cleanup)
                guard let callback = self.callback, let userData = userData else {
                    // Callback was cleared, ignore this reading
                    return
                }
                
                // Callback might crash if Rust side has issues, but we can't catch that
                // However, we've already validated all inputs, so it should be safe
                callback(x, y, z, userData)
            }
        }
        
        isActive = true
        print("[MagnetometerListener] Started magnetometer updates")
        #else
        throw NSError(
            domain: "MagnetometerListener",
            code: 2,
            userInfo: [NSLocalizedDescriptionKey: "CoreMotion framework not available"]
        )
        #endif
    }
    
    func stop() {
        guard isActive else {
            return
        }
        
        // Clear callback FIRST to prevent any new callbacks from being invoked
        // This must happen before stopping updates to avoid race conditions
        self.callback = nil
        self.callbackUserData = nil
        
        // Set inactive to prevent any in-flight callbacks from processing
        isActive = false
        
        #if canImport(CoreMotion)
        // Stop updates - this will prevent new callbacks from being queued
        if let manager = motionManager as? CMMotionManager {
            manager.stopMagnetometerUpdates()
        }
        #endif
        
        print("[MagnetometerListener] Stopped magnetometer updates")
    }
    
    deinit {
        stop()
    }
}

// C-compatible FFI functions for Rust

@_cdecl("xos_sensors_magnetometer_init")
func xos_sensors_magnetometer_init() -> UInt32 {
    // Wrap in autoreleasepool to prevent memory issues
    return autoreleasepool {
        // Use do-catch to prevent any Swift exceptions from crashing
        do {
            guard let listenerId = MagnetometerListenerManager.shared.createListener() else {
                print("[xos_sensors_magnetometer_init] Failed to create listener")
                return UInt32.max
            }
            print("[xos_sensors_magnetometer_init] Successfully created listener with ID: \(listenerId)")
            return listenerId
        } catch {
            print("[xos_sensors_magnetometer_init] Exception: \(error.localizedDescription)")
            return UInt32.max
        }
    }
}

@_cdecl("xos_sensors_magnetometer_set_callback")
func xos_sensors_magnetometer_set_callback(
    _ magnetometerId: UInt32,
    _ callback: MagnetometerCallback?,
    _ userData: UnsafeMutableRawPointer?
) {
    // Wrap in autoreleasepool and do-catch to prevent crashes
    autoreleasepool {
        do {
            guard let listener = MagnetometerListenerManager.shared.getListener(magnetometerId) else {
                print("[xos_sensors_magnetometer_set_callback] Listener not found: \(magnetometerId)")
                return
            }
            
            if let callback = callback {
                listener.setCallback(callback, userData: userData)
            }
        } catch {
            print("[xos_sensors_magnetometer_set_callback] Exception: \(error.localizedDescription)")
        }
    }
}

@_cdecl("xos_sensors_magnetometer_start")
func xos_sensors_magnetometer_start(_ magnetometerId: UInt32) -> Int32 {
    // Wrap in autoreleasepool and do-catch to prevent crashes
    return autoreleasepool {
        do {
            guard let listener = MagnetometerListenerManager.shared.getListener(magnetometerId) else {
                print("[xos_sensors_magnetometer_start] Listener not found: \(magnetometerId)")
                return 1
            }
            
            try listener.start()
            return 0
        } catch {
            print("[xos_sensors_magnetometer_start] Error: \(error.localizedDescription)")
            // Don't crash - just return error code
            return 1
        }
    }
}

@_cdecl("xos_sensors_magnetometer_destroy")
func xos_sensors_magnetometer_destroy(_ magnetometerId: UInt32) {
    // Wrap in autoreleasepool and do-catch to prevent crashes
    autoreleasepool {
        do {
            MagnetometerListenerManager.shared.destroyListener(magnetometerId)
        } catch {
            print("[xos_sensors_magnetometer_destroy] Exception: \(error.localizedDescription)")
        }
    }
}

