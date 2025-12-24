import Foundation
import AVFoundation

// Shared audio engine manager for microphone
final class SharedAudioEngine {
    static let shared = SharedAudioEngine()
    
    private var engine: AVAudioEngine?
    private let lock = NSLock()
    
    private init() {}
    
    func getOrCreateEngine() -> AVAudioEngine {
        lock.lock()
        defer { lock.unlock() }
        
        if engine == nil {
            engine = AVAudioEngine()
            print("[SharedAudioEngine] Created new AVAudioEngine")
        }
        return engine!
    }
    
    func configureAudioSession() throws {
        let audioSession = AVAudioSession.sharedInstance()
        
        // Set category for recording
        try audioSession.setCategory(.record, mode: .measurement, options: [])
        try audioSession.setActive(true)
        print("[SharedAudioEngine] Audio session configured for recording")
    }
    
    func stopEngine() {
        lock.lock()
        defer { lock.unlock() }
        
        if let engine = engine, engine.isRunning {
            engine.stop()
            print("[SharedAudioEngine] Engine stopped")
        }
    }
}

// Audio listener manager
final class AudioListenerManager {
    static let shared = AudioListenerManager()
    
    private var listeners: [UInt32: AudioListener] = [:]
    private var nextListenerId: UInt32 = 0
    private let lock = NSLock()
    
    private init() {}
    
    func createListener(deviceId: UInt32, sampleRate: Double, channels: UInt32, bufferDuration: Double) -> UInt32? {
        lock.lock()
        defer { lock.unlock() }
        
        let listenerId = nextListenerId
        nextListenerId += 1
        
        do {
            print("[AudioListenerManager] Creating listener ID=\(listenerId), deviceId=\(deviceId), sampleRate=\(sampleRate), channels=\(channels)")
            let listener = try AudioListener(
                deviceId: deviceId,
                listenerId: listenerId,
                sampleRate: sampleRate,
                channels: channels,
                bufferDuration: bufferDuration
            )
            listeners[listenerId] = listener
            print("[AudioListenerManager] Successfully created listener ID=\(listenerId)")
            return listenerId
        } catch {
            print("[AudioListenerManager] Failed to create listener ID=\(listenerId): \(error.localizedDescription)")
            if let nsError = error as NSError? {
                print("[AudioListenerManager] Error domain: \(nsError.domain), code: \(nsError.code)")
                let userInfo = nsError.userInfo
                if !userInfo.isEmpty {
                    print("[AudioListenerManager] Error userInfo: \(userInfo)")
                }
            }
            return nil
        }
    }
    
    func getListener(_ id: UInt32) -> AudioListener? {
        lock.lock()
        defer { lock.unlock() }
        return listeners[id]
    }
    
    func destroyListener(_ id: UInt32) {
        lock.lock()
        defer { lock.unlock() }
        listeners.removeValue(forKey: id)
    }
}

// Audio listener implementation
final class AudioListener {
    private let listenerId: UInt32
    private var engine: AVAudioEngine
    private var inputNode: AVAudioInputNode?
    private var callback: AudioCallback?
    private var callbackUserData: UnsafeMutableRawPointer?
    private let sampleRate: Double
    private let channels: UInt32
    
    init(deviceId: UInt32, listenerId: UInt32, sampleRate: Double, channels: UInt32, bufferDuration: Double) throws {
        self.listenerId = listenerId
        self.sampleRate = sampleRate
        self.channels = channels
        
        // Create a dedicated engine for this listener
        self.engine = AVAudioEngine()
        print("[AudioListener] Created AVAudioEngine for listener ID=\(listenerId)")
        
        // Check and request microphone permission
        let audioSession = AVAudioSession.sharedInstance()
        let currentStatus = audioSession.recordPermission
        print("[AudioListener] Current permission status: \(currentStatus.rawValue)")
        
        // If already denied, fail with clear message
        if currentStatus == .denied {
            print("[AudioListener] Permission already denied")
            throw NSError(
                domain: "AudioListener",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "Microphone permission denied. Please enable it in Settings > Privacy & Security > Microphone > XOS"]
            )
        }
        
        // If not yet determined, request permission synchronously
        if currentStatus != .granted {
            print("[AudioListener] Requesting microphone permission...")
            
            var permissionGranted = false
            let semaphore = DispatchSemaphore(value: 0)
            
            audioSession.requestRecordPermission { granted in
                print("[AudioListener] Permission result: \(granted)")
                permissionGranted = granted
                semaphore.signal()
            }
            
            // Wait for permission response (10 second timeout)
            _ = semaphore.wait(timeout: .now() + 10.0)
            
            if !permissionGranted {
                print("[AudioListener] Permission denied by user")
                throw NSError(
                    domain: "AudioListener",
                    code: 1,
                    userInfo: [NSLocalizedDescriptionKey: "Microphone permission denied by user"]
                )
            }
            
            print("[AudioListener] Permission granted!")
        } else {
            print("[AudioListener] Permission already granted, proceeding...")
        }
        
        // Configure audio session
        try SharedAudioEngine.shared.configureAudioSession()
        
        // Setup audio engine
        let inputNode = engine.inputNode
        self.inputNode = inputNode
        
        // Get the actual input format
        let inputFormat = inputNode.outputFormat(forBus: 0)
        print("[AudioListener] inputNode format: sampleRate=\(inputFormat.sampleRate), channels=\(inputFormat.channelCount)")
        
        // Install tap on input node
        let bufferSize: AVAudioFrameCount = 4096
        print("[AudioListener] Installing tap with bufferSize=\(bufferSize)")
        
        inputNode.installTap(onBus: 0, bufferSize: bufferSize, format: inputFormat) { [weak self] (buffer, time) in
            guard let self = self else { return }
            
            // Get channel data (non-interleaved format)
            guard let channelData = buffer.floatChannelData else {
                return
            }
            
            let frameLength = Int(buffer.frameLength)
            let channelCount = Int(buffer.format.channelCount)
            
            // Safety check
            guard frameLength > 0 && channelCount > 0 else {
                return
            }
            
            // Extract samples - for mono, just use first channel
            // For multi-channel, interleave them for the callback
            var samples: [Float] = []
            samples.reserveCapacity(frameLength * channelCount)
            
            if channelCount == 1 {
                // Mono: just read from first channel
                let channelPtr = channelData[0]
                samples = Array(UnsafeBufferPointer(start: channelPtr, count: frameLength))
            } else {
                // Multi-channel: interleave samples
                for frame in 0..<frameLength {
                    for channel in 0..<channelCount {
                        samples.append(channelData[channel][frame])
                    }
                }
            }
            
            // Call the Rust callback if set
            if let callback = self.callback, !samples.isEmpty {
                samples.withUnsafeBufferPointer { ptr in
                    if let baseAddress = ptr.baseAddress {
                        callback(baseAddress, ptr.count, self.callbackUserData)
                    }
                }
            }
        }
        
        // Start the engine
        try engine.start()
        print("[AudioListener] Engine started successfully")
    }
    
    func setCallback(_ callback: @escaping AudioCallback, userData: UnsafeMutableRawPointer?) {
        self.callback = callback
        self.callbackUserData = userData
    }
    
    func start() throws {
        if !engine.isRunning {
            try engine.start()
            print("[AudioListener] Engine started")
        }
    }
    
    func pause() {
        if engine.isRunning {
            engine.pause()
            print("[AudioListener] Engine paused")
        }
    }
    
    deinit {
        print("[AudioListener] Deinitializing listener ID=\(listenerId)")
        
        // Remove tap first
        inputNode?.removeTap(onBus: 0)
        
        // Stop engine
        if engine.isRunning {
            engine.stop()
        }
        
        inputNode = nil
        
        print("[AudioListener] Cleanup complete for listener ID=\(listenerId)")
    }
}

// C-compatible FFI functions for Rust

@_cdecl("xos_audio_get_device_count")
func xos_audio_get_device_count() -> UInt32 {
    // For now, we only support the built-in microphone
    return 1
}

@_cdecl("xos_audio_get_device_name")
func xos_audio_get_device_name(_ deviceId: UInt32) -> UnsafePointer<CChar>? {
    // For now, we only support the built-in microphone
    if deviceId == 0 {
        let name = "Built-in Microphone"
        if let mutablePtr = strdup(name) {
            // Convert mutable pointer to immutable pointer
            return UnsafePointer<CChar>(mutablePtr) // Caller must free with xos_audio_free_string
        }
    }
    return nil
}

@_cdecl("xos_audio_device_is_input")
func xos_audio_device_is_input(_ deviceId: UInt32) -> Int32 {
    // For now, we only support input (microphone)
    if deviceId == 0 {
        return 1
    }
    return 0
}

@_cdecl("xos_audio_device_is_output")
func xos_audio_device_is_output(_ deviceId: UInt32) -> Int32 {
    // For now, we don't support output
    return 0
}

@_cdecl("xos_audio_free_string")
func xos_audio_free_string(_ ptr: UnsafePointer<CChar>?) {
    guard let ptr = ptr else { return }
    free(UnsafeMutablePointer(mutating: ptr))
}

@_cdecl("xos_audio_listener_init")
func xos_audio_listener_init(
    _ deviceId: UInt32,
    _ sampleRate: Double,
    _ channels: UInt32,
    _ bufferDuration: Double
) -> UInt32 {
    guard let listenerId = AudioListenerManager.shared.createListener(
        deviceId: deviceId,
        sampleRate: sampleRate,
        channels: channels,
        bufferDuration: bufferDuration
    ) else {
        print("[xos_audio_listener_init] Failed to create listener (createListener returned nil)")
        return UInt32.max
    }
    print("[xos_audio_listener_init] Successfully created listener with ID: \(listenerId)")
    return listenerId
}

// Callback type for Rust (non-optional pointers to match Rust signature)
typealias AudioCallback = @convention(c) (UnsafePointer<Float>, Int, UnsafeMutableRawPointer?) -> Void

@_cdecl("xos_audio_listener_set_callback")
func xos_audio_listener_set_callback(
    _ listenerId: UInt32,
    _ callback: AudioCallback?,
    _ userData: UnsafeMutableRawPointer?
) {
    guard let listener = AudioListenerManager.shared.getListener(listenerId) else {
        return
    }
    
    if let callback = callback {
        listener.setCallback(callback, userData: userData)
    }
}

@_cdecl("xos_audio_listener_start")
func xos_audio_listener_start(_ listenerId: UInt32) -> Int32 {
    guard let listener = AudioListenerManager.shared.getListener(listenerId) else {
        return 1
    }
    
    do {
        try listener.start()
        return 0
    } catch {
        print("[xos_audio_listener_start] Error: \(error.localizedDescription)")
        return 1
    }
}

@_cdecl("xos_audio_listener_pause")
func xos_audio_listener_pause(_ listenerId: UInt32) -> Int32 {
    guard let listener = AudioListenerManager.shared.getListener(listenerId) else {
        return 1
    }
    
    listener.pause()
    return 0
}

@_cdecl("xos_audio_listener_destroy")
func xos_audio_listener_destroy(_ listenerId: UInt32) {
    AudioListenerManager.shared.destroyListener(listenerId)
}

