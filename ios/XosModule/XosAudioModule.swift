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
    
    func withEngineModification<T>(_ block: (AVAudioEngine) throws -> T) throws -> T {
        lock.lock()
        defer { lock.unlock() }
        
        guard let engine = engine else {
            throw NSError(
                domain: "SharedAudioEngine",
                code: 2,
                userInfo: [NSLocalizedDescriptionKey: "Engine must exist before modification"]
            )
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
            let semaphore = DispatchSemaphore(value: 0)
            var permissionGranted = false
            var permissionError: Error?
            
            // Request permission - must be on main thread
            // Use a background queue to wait, but request on main thread
            let requestPermission = {
                audioSession.requestRecordPermission { granted in
                    print("[AudioListener] requestRecordPermission callback, granted=\(granted)")
                    permissionGranted = granted
                    if !granted {
                        permissionError = NSError(
                            domain: "AudioListener",
                            code: 1,
                            userInfo: [NSLocalizedDescriptionKey: "Microphone permission denied by user"]
                        )
                    }
                    semaphore.signal()
                }
            }
            
            // Always dispatch permission request to main thread
            // Then wait on current thread (which might be background)
            DispatchQueue.main.async {
                requestPermission()
            }
            
            // Wait for permission response (with timeout)
            // If we're on main thread, this will deadlock, so we need to handle it differently
            if Thread.isMainThread {
                // On main thread - use RunLoop to process events while waiting
                let deadline = Date(timeIntervalSinceNow: 10.0)
                var timedOut = true
                
                while Date() < deadline {
                    // Check if semaphore is available without blocking
                    let result = semaphore.wait(timeout: .now() + 0.1)
                    if result == .success {
                        timedOut = false
                        break
                    }
                    // Process run loop events to allow async blocks and callbacks to execute
                    RunLoop.current.run(mode: .default, before: Date(timeIntervalSinceNow: 0.1))
                }
                
                if timedOut {
                    throw NSError(
                        domain: "AudioListener",
                        code: 3,
                        userInfo: [NSLocalizedDescriptionKey: "Microphone permission request timed out"]
                    )
                }
            } else {
                // Not on main thread - can safely wait on semaphore
                let timeout = semaphore.wait(timeout: .now() + 10.0)
                if timeout == .timedOut {
                    throw NSError(
                        domain: "AudioListener",
                        code: 3,
                        userInfo: [NSLocalizedDescriptionKey: "Microphone permission request timed out"]
                    )
                }
            }
            
            // Check results
            if let error = permissionError {
                throw error
            }
            if !permissionGranted {
                throw NSError(
                    domain: "AudioListener",
                    code: 1,
                    userInfo: [NSLocalizedDescriptionKey: "Microphone permission denied"]
                )
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
        do {
            try SharedAudioEngine.shared.withEngineModification { engine in
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
                    guard let self = self else { return }
                    
                    // Get channel data (non-interleaved format)
                    guard let channelData = buffer.floatChannelData else {
                        print("[AudioListener] Warning: buffer.floatChannelData is nil")
                        return
                    }
                    
                    let frameLength = Int(buffer.frameLength)
                    let channelCount = Int(buffer.format.channelCount)
                    
                    // Safety check
                    guard frameLength > 0 && channelCount > 0 else {
                        print("[AudioListener] Warning: invalid frameLength=\(frameLength) or channelCount=\(channelCount)")
                        return
                    }
                    
                    // Extract samples - for mono, just use first channel
                    // For multi-channel, we'll interleave them for the callback
                    // The callback expects: [ch0_sample0, ch1_sample0, ch0_sample1, ch1_sample1, ...]
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
            }
        } catch {
            SharedAudioEngine.shared.releaseMicrophone()
            throw NSError(
                domain: "AudioListener",
                code: 4,
                userInfo: [NSLocalizedDescriptionKey: "Failed to configure audio engine: \(error.localizedDescription)"]
            )
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

