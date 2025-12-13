import Foundation
import AVFoundation

// Shared audio engine manager for microphone
final class SharedAudioEngine {
    static let shared = SharedAudioEngine()
    
    private var engine: AVAudioEngine?
    private let lock = NSLock()
    private var microphoneRefCount = 0
    
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
    
    func acquireMicrophone() {
        lock.lock()
        defer { lock.unlock() }
        
        microphoneRefCount += 1
        print("[SharedAudioEngine] Microphone acquired (refCount: \(microphoneRefCount))")
        
        if engine == nil {
            engine = AVAudioEngine()
        }
    }
    
    func startEngineIfNeeded() throws {
        lock.lock()
        defer { lock.unlock() }
        
        guard let engine = engine else {
            throw NSError(domain: "SharedAudioEngine", code: 1, userInfo: [NSLocalizedDescriptionKey: "Engine not created"])
        }
        
        if !engine.isRunning {
            try engine.start()
            print("[SharedAudioEngine] Engine started")
        }
    }
    
    func withEngineModification<T>(_ block: (AVAudioEngine) throws -> T) rethrows -> T {
        lock.lock()
        defer { lock.unlock() }
        
        guard let engine = engine else {
            fatalError("Engine must exist before modification")
        }
        
        let wasRunning = engine.isRunning
        if wasRunning {
            engine.stop()
            print("[SharedAudioEngine] Engine stopped for graph modification")
        }
        
        defer {
            if wasRunning {
                do {
                    try engine.start()
                    print("[SharedAudioEngine] Engine restarted after graph modification")
                } catch {
                    print("[SharedAudioEngine] Failed to restart engine after modification: \(error.localizedDescription)")
                }
            }
        }
        
        return try block(engine)
    }
    
    func releaseMicrophone() {
        lock.lock()
        defer { lock.unlock() }
        
        microphoneRefCount = max(0, microphoneRefCount - 1)
        print("[SharedAudioEngine] Microphone released (refCount: \(microphoneRefCount))")
        
        if microphoneRefCount == 0 {
            if let engine = engine, engine.isRunning {
                engine.stop()
                engine.reset()
                print("[SharedAudioEngine] Engine stopped (no active modules)")
            }
        }
    }
    
    func configureAudioSession() throws {
        let audioSession = AVAudioSession.sharedInstance()
        
        // Try to deactivate first to ensure clean state
        try? audioSession.setActive(false)
        
        // Set category for recording
        try audioSession.setCategory(.record, mode: .default, options: [])
        try audioSession.setActive(true)
        print("[SharedAudioEngine] Audio session configured for record")
    }
    
    func deactivateAudioSessionIfNeeded() {
        lock.lock()
        defer { lock.unlock() }
        
        // Only deactivate if microphone is inactive
        if microphoneRefCount == 0 {
            do {
                try AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
                print("[SharedAudioEngine] Audio session deactivated")
            } catch {
                // Ignore errors when deactivating
            }
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
            let listener = try AudioListener(
                deviceId: deviceId,
                listenerId: listenerId,
                sampleRate: sampleRate,
                channels: channels,
                bufferDuration: bufferDuration
            )
            listeners[listenerId] = listener
            return listenerId
        } catch {
            print("[AudioListenerManager] Failed to create listener: \(error.localizedDescription)")
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
    private var inputNode: AVAudioInputNode?
    private var inputFormat: AVAudioFormat?
    private var sampleBuffer: [Float] = []
    private let queue = DispatchQueue(label: "com.xos.audio.buffer")
    private var callback: AudioCallback?
    private var callbackUserData: UnsafeMutableRawPointer?
    private let sampleRate: Double
    private let channels: UInt32
    private let bufferDuration: Double
    
    init(deviceId: UInt32, listenerId: UInt32, sampleRate: Double, channels: UInt32, bufferDuration: Double) throws {
        self.listenerId = listenerId
        self.sampleRate = sampleRate
        self.channels = channels
        self.bufferDuration = bufferDuration
        
        // Request microphone permission first
        let audioSession = AVAudioSession.sharedInstance()
        
        // Check current permission status
        let currentStatus = audioSession.recordPermission
        
        // If already denied, fail fast
        if currentStatus == .denied {
            throw NSError(
                domain: "AudioListener",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "Microphone permission has been denied in Settings"]
            )
        }
        
        // If not already granted, request permission
        if currentStatus != .granted {
            if Thread.isMainThread {
                audioSession.requestRecordPermission { granted in
                    print("[AudioListener] requestRecordPermission callback (main-thread path), granted=\(granted)")
                }
                throw NSError(
                    domain: "AudioListener",
                    code: 2,
                    userInfo: [NSLocalizedDescriptionKey: "Microphone permission request in progress; please retry after the user responds"]
                )
            } else {
                let semaphore = DispatchSemaphore(value: 0)
                var permissionGranted = false
                
                DispatchQueue.main.async {
                    audioSession.requestRecordPermission { granted in
                        print("[AudioListener] requestRecordPermission callback, granted=\(granted)")
                        permissionGranted = granted
                        semaphore.signal()
                    }
                }
                
                // Wait for permission response (with timeout)
                let timeout = semaphore.wait(timeout: .now() + 10.0)
                if timeout == .timedOut || !permissionGranted {
                    throw NSError(
                        domain: "AudioListener",
                        code: 1,
                        userInfo: [NSLocalizedDescriptionKey: "Microphone permission denied or request timed out"]
                    )
                }
            }
        }
        
        // Configure audio session
        do {
            try SharedAudioEngine.shared.configureAudioSession()
        } catch {
            // Fallback: try without options if it fails
            print("[AudioListener] AVAudioSession primary configuration failed: \(error.localizedDescription)")
            do {
                let audioSession = AVAudioSession.sharedInstance()
                try? audioSession.setActive(false)
                try audioSession.setCategory(.record, mode: .default)
                try audioSession.setActive(true)
            } catch {
                try? AVAudioSession.sharedInstance().setActive(true)
            }
        }
        
        // Use shared audio engine
        SharedAudioEngine.shared.acquireMicrophone()
        
        // Safely modify engine graph
        SharedAudioEngine.shared.withEngineModification { engine in
            let inputNode = engine.inputNode
            self.inputNode = inputNode
            
            // Get the actual input format
            let inputFormat = inputNode.outputFormat(forBus: 0)
            print("[AudioListener] inputNode.outputFormat: sampleRate=\(inputFormat.sampleRate), channels=\(inputFormat.channelCount)")
            
            self.inputFormat = inputFormat
            
            // Clear sample buffer
            queue.async {
                self.sampleBuffer.removeAll()
            }
            
            // Install tap on input node
            let bufferSize: AVAudioFrameCount = 4096
            print("[AudioListener] installing tap with bufferSize=\(bufferSize)")
            
            // Remove any existing tap first
            inputNode.removeTap(onBus: 0)
            
            inputNode.installTap(onBus: 0, bufferSize: bufferSize, format: inputFormat) { [weak self] (buffer, time) in
                guard let self = self, let channelData = buffer.floatChannelData else { return }
                
                let frameLength = Int(buffer.frameLength)
                let channelCount = Int(buffer.format.channelCount)
                
                // Extract samples from buffer (non-interleaved format)
                var samples: [Float] = []
                if channelCount == 1 {
                    // Mono: just read from first channel
                    samples = Array(UnsafeBufferPointer(start: channelData[0], count: frameLength))
                } else {
                    // Multi-channel: take first channel for now
                    samples = Array(UnsafeBufferPointer(start: channelData[0], count: frameLength))
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
        }
        
        // Ensure engine is running
        try SharedAudioEngine.shared.startEngineIfNeeded()
        print("[AudioListener] initialized successfully")
    }
    
    func setCallback(_ callback: @escaping AudioCallback, userData: UnsafeMutableRawPointer?) {
        self.callback = callback
        self.callbackUserData = userData
    }
    
    func start() throws {
        // Already started in init, but we can check if engine is running
        try SharedAudioEngine.shared.startEngineIfNeeded()
    }
    
    func pause() {
        // Pause by pausing the engine
        let engine = SharedAudioEngine.shared.getOrCreateEngine()
        if engine.isRunning {
            engine.pause()
        }
    }
    
    deinit {
        inputNode?.removeTap(onBus: 0)
        inputNode = nil
        inputFormat = nil
        SharedAudioEngine.shared.releaseMicrophone()
        SharedAudioEngine.shared.deactivateAudioSessionIfNeeded()
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
        return UInt32.max
    }
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

