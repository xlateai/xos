import Foundation
import AVFoundation

// Shared audio session manager for both microphone and speakers
final class SharedAudioSession {
    static let shared = SharedAudioSession()
    
    private var isConfigured = false
    private var hasPlayback = false
    private var hasRecording = false
    private let lock = NSLock()
    
    private init() {}
    
    /// Configure audio session with the appropriate category
    /// This should be called ONCE with the correct mode
    func configureForRecording() throws {
        lock.lock()
        defer { lock.unlock() }
        
        hasRecording = true
        
        let audioSession = AVAudioSession.sharedInstance()
        
        // Check current category - if it's already playAndRecord, don't downgrade!
        let currentCategory = audioSession.category
        print("[SharedAudioSession] Current category: \(currentCategory)")
        
        // If already configured with playAndRecord, keep it (don't downgrade)
        if currentCategory == .playAndRecord {
            print("[SharedAudioSession] ✅ Already .playAndRecord, keeping it")
            isConfigured = true
            return
        }
        
        // If already configured with .record and we don't need playback, skip
        if currentCategory == .record && !hasPlayback && isConfigured {
            print("[SharedAudioSession] Already configured for .record, skipping")
            return
        }
        
        // Choose category based on whether we need playback too
        if hasPlayback {
            // Need both recording and playback
            try audioSession.setCategory(.playAndRecord, mode: .default, options: [.defaultToSpeaker, .allowBluetooth])
            print("[SharedAudioSession] ✅ Configured for RECORDING + PLAYBACK (.playAndRecord)")
        } else {
            // Recording only - but only set if not already in a compatible mode
            if currentCategory != .record && currentCategory != .playAndRecord {
                try audioSession.setCategory(.record, mode: .measurement, options: [])
                print("[SharedAudioSession] ✅ Configured for RECORDING only (.record)")
            } else {
                print("[SharedAudioSession] ✅ Keeping existing category: \(currentCategory)")
            }
        }
        
        try audioSession.setActive(true)
        isConfigured = true
    }
    
    /// Configure audio session for playback
    func configureForPlayback() throws {
        lock.lock()
        defer { lock.unlock() }
        
        hasPlayback = true
        
        let audioSession = AVAudioSession.sharedInstance()
        
        // If we already have recording, upgrade to playAndRecord
        if hasRecording && isConfigured {
            // Upgrade existing .record to .playAndRecord
            try audioSession.setCategory(.playAndRecord, mode: .default, options: [.defaultToSpeaker, .allowBluetooth])
            print("[SharedAudioSession] ⬆️  UPGRADED to .playAndRecord (was .record)")
            try audioSession.setActive(true)
        } else if !isConfigured {
            // First time setup - just playback
            try audioSession.setCategory(.playAndRecord, mode: .default, options: [.defaultToSpeaker, .allowBluetooth])
            print("[SharedAudioSession] ✅ Configured for PLAYBACK (.playAndRecord)")
            try audioSession.setActive(true)
            isConfigured = true
        } else {
            print("[SharedAudioSession] Already configured for playback, skipping")
        }
    }
    
    /// Reset configuration state (for testing/debugging)
    func reset() {
        lock.lock()
        defer { lock.unlock() }
        isConfigured = false
        hasPlayback = false
        hasRecording = false
        print("[SharedAudioSession] Reset configuration state")
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
            
            // Wait for permission response with proper main thread handling
            if Thread.isMainThread {
                // On main thread - process run loop while waiting
                print("[AudioListener] Waiting for permission on main thread...")
                let deadline = Date(timeIntervalSinceNow: 15.0)
                while Date() < deadline {
                    if semaphore.wait(timeout: .now()) == .success {
                        break
                    }
                    // Process run loop to allow callbacks to execute
                    RunLoop.current.run(mode: .default, before: Date(timeIntervalSinceNow: 0.01))
                }
            } else {
                // Not on main thread - can safely block
                print("[AudioListener] Waiting for permission on background thread...")
                _ = semaphore.wait(timeout: .now() + 15.0)
            }
            
            if !permissionGranted {
                print("[AudioListener] Permission denied or timed out")
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
        
        // Configure audio session (will upgrade to playAndRecord if needed)
        try SharedAudioSession.shared.configureForRecording()
        
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
    // We support both built-in microphone (input) and speaker (output)
    return 2
}

@_cdecl("xos_audio_get_device_name")
func xos_audio_get_device_name(_ deviceId: UInt32) -> UnsafePointer<CChar>? {
    let name: String
    if deviceId == 0 {
        name = "Built-in Microphone"
    } else if deviceId == 1 {
        name = "Built-in Speaker"
    } else {
        return nil
    }
    
    if let mutablePtr = strdup(name) {
        // Convert mutable pointer to immutable pointer
        return UnsafePointer<CChar>(mutablePtr) // Caller must free with xos_audio_free_string
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
    // Device ID 1 is the built-in speaker (output)
    if deviceId == 1 {
        return 1
    }
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

// MARK: - Audio Player (Speaker Output)

// Audio player manager
final class AudioPlayerManager {
    static let shared = AudioPlayerManager()
    
    private var players: [UInt32: AudioPlayer] = [:]
    private var nextPlayerId: UInt32 = 0
    private let lock = NSLock()
    
    private init() {}
    
    func createPlayer(deviceId: UInt32, sampleRate: Double, channels: UInt32) -> UInt32? {
        lock.lock()
        defer { lock.unlock() }
        
        let playerId = nextPlayerId
        nextPlayerId += 1
        
        do {
            print("[AudioPlayerManager] Creating player ID=\(playerId), deviceId=\(deviceId), sampleRate=\(sampleRate), channels=\(channels)")
            let player = try AudioPlayer(
                deviceId: deviceId,
                playerId: playerId,
                sampleRate: sampleRate,
                channels: channels
            )
            players[playerId] = player
            print("[AudioPlayerManager] Successfully created player ID=\(playerId)")
            return playerId
        } catch {
            print("[AudioPlayerManager] Failed to create player ID=\(playerId): \(error.localizedDescription)")
            return nil
        }
    }
    
    func getPlayer(_ id: UInt32) -> AudioPlayer? {
        lock.lock()
        defer { lock.unlock() }
        return players[id]
    }
    
    func destroyPlayer(_ id: UInt32) {
        lock.lock()
        defer { lock.unlock() }
        players.removeValue(forKey: id)
    }
}

// Audio player implementation for speaker output
final class AudioPlayer {
    private let playerId: UInt32
    private var engine: AVAudioEngine
    private var playerNode: AVAudioPlayerNode
    private let sampleRate: Double
    private let channels: UInt32
    private let format: AVAudioFormat
    private var queuedSampleCount: Int = 0
    private let lock = NSLock()
    
    init(deviceId: UInt32, playerId: UInt32, sampleRate: Double, channels: UInt32) throws {
        self.playerId = playerId
        self.sampleRate = sampleRate
        self.channels = channels
        
        // Create audio engine and player node
        self.engine = AVAudioEngine()
        self.playerNode = AVAudioPlayerNode()
        
        print("[AudioPlayer] Created AVAudioEngine and AVAudioPlayerNode for player ID=\(playerId)")
        
        // Configure audio session for playback (will upgrade to playAndRecord if recording exists)
        try SharedAudioSession.shared.configureForPlayback()
        
        // Create audio format
        guard let format = AVAudioFormat(
            commonFormat: .pcmFormatFloat32,
            sampleRate: sampleRate,
            channels: AVAudioChannelCount(channels),
            interleaved: false
        ) else {
            throw NSError(
                domain: "AudioPlayer",
                code: 2,
                userInfo: [NSLocalizedDescriptionKey: "Failed to create audio format"]
            )
        }
        self.format = format
        
        // Attach player node to engine
        engine.attach(playerNode)
        
        // Connect player node to output
        engine.connect(playerNode, to: engine.mainMixerNode, format: format)
        
        // Start the engine
        try engine.start()
        print("[AudioPlayer] Engine started successfully")
    }
    
    func queueSamples(_ samples: UnsafePointer<Float>, count: Int) throws {
        lock.lock()
        defer { lock.unlock() }
        
        guard count > 0 else { return }
        
        // Create PCM buffer
        guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(count)) else {
            throw NSError(
                domain: "AudioPlayer",
                code: 3,
                userInfo: [NSLocalizedDescriptionKey: "Failed to create PCM buffer"]
            )
        }
        
        buffer.frameLength = AVAudioFrameCount(count)
        
        // Copy samples to buffer (non-interleaved format)
        guard let channelData = buffer.floatChannelData else {
            throw NSError(
                domain: "AudioPlayer",
                code: 4,
                userInfo: [NSLocalizedDescriptionKey: "Failed to get channel data"]
            )
        }
        
        // For mono, just copy directly
        if channels == 1 {
            channelData[0].update(from: samples, count: count)
        } else {
            // For multi-channel, samples are interleaved, so we need to de-interleave
            for frame in 0..<count / Int(channels) {
                for channel in 0..<Int(channels) {
                    channelData[channel][frame] = samples[frame * Int(channels) + channel]
                }
            }
        }
        
        // Schedule buffer for playback
        playerNode.scheduleBuffer(buffer, completionHandler: nil)
        queuedSampleCount += count
        
        // Start playing if not already playing
        if !playerNode.isPlaying {
            playerNode.play()
            print("[AudioPlayer] Started playback")
        }
    }
    
    func getBufferSize() -> Int {
        lock.lock()
        defer { lock.unlock() }
        return queuedSampleCount
    }
    
    func start() throws {
        if !playerNode.isPlaying {
            playerNode.play()
            print("[AudioPlayer] Player started")
        }
    }
    
    func stop() {
        if playerNode.isPlaying {
            playerNode.stop()
            print("[AudioPlayer] Player stopped")
        }
    }
    
    deinit {
        print("[AudioPlayer] Deinitializing player ID=\(playerId)")
        
        // Stop player
        playerNode.stop()
        
        // Stop engine
        if engine.isRunning {
            engine.stop()
        }
        
        // Detach node
        engine.detach(playerNode)
        
        print("[AudioPlayer] Cleanup complete for player ID=\(playerId)")
    }
}

// C-compatible FFI functions for audio player

@_cdecl("xos_audio_player_init")
func xos_audio_player_init(
    _ deviceId: UInt32,
    _ sampleRate: Double,
    _ channels: UInt32
) -> UInt32 {
    guard let playerId = AudioPlayerManager.shared.createPlayer(
        deviceId: deviceId,
        sampleRate: sampleRate,
        channels: channels
    ) else {
        print("[xos_audio_player_init] Failed to create player")
        return UInt32.max
    }
    print("[xos_audio_player_init] Successfully created player with ID: \(playerId)")
    return playerId
}

@_cdecl("xos_audio_player_queue_samples")
func xos_audio_player_queue_samples(
    _ playerId: UInt32,
    _ samples: UnsafePointer<Float>,
    _ count: Int
) -> Int32 {
    guard let player = AudioPlayerManager.shared.getPlayer(playerId) else {
        return 1
    }
    
    do {
        try player.queueSamples(samples, count: count)
        return 0
    } catch {
        print("[xos_audio_player_queue_samples] Error: \(error.localizedDescription)")
        return 1
    }
}

@_cdecl("xos_audio_player_get_buffer_size")
func xos_audio_player_get_buffer_size(_ playerId: UInt32) -> UInt32 {
    guard let player = AudioPlayerManager.shared.getPlayer(playerId) else {
        return 0
    }
    return UInt32(player.getBufferSize())
}

@_cdecl("xos_audio_player_start")
func xos_audio_player_start(_ playerId: UInt32) -> Int32 {
    guard let player = AudioPlayerManager.shared.getPlayer(playerId) else {
        return 1
    }
    
    do {
        try player.start()
        return 0
    } catch {
        print("[xos_audio_player_start] Error: \(error.localizedDescription)")
        return 1
    }
}

@_cdecl("xos_audio_player_stop")
func xos_audio_player_stop(_ playerId: UInt32) -> Int32 {
    guard let player = AudioPlayerManager.shared.getPlayer(playerId) else {
        return 1
    }
    
    player.stop()
    return 0
}

@_cdecl("xos_audio_player_destroy")
func xos_audio_player_destroy(_ playerId: UInt32) {
    AudioPlayerManager.shared.destroyPlayer(playerId)
}

