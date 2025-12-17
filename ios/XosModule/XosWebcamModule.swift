import Foundation
import AVFoundation
import CoreVideo

// Camera info structure for FFI
struct CameraInfo {
    let id: String
    let name: String
    let position: String // "back" or "front"
    let deviceType: String // "wide", "ultrawide", "telephoto", etc.
}

// Shared camera manager
final class WebcamManager: NSObject {
    static let shared = WebcamManager()
    
    private var captureSession: AVCaptureSession?
    private var videoOutput: AVCaptureVideoDataOutput?
    private var videoInput: AVCaptureDeviceInput?
    private var latestPixelBuffer: CVPixelBuffer?
    private var latestResolution: (width: UInt32, height: UInt32) = (0, 0)
    private let lock = NSLock()
    private var isInitialized = false
    private var isInitializing = false
    private var availableCameras: [CameraInfo] = []
    private var currentCameraIndex: Int = 0
    
    private override init() {
        super.init()
    }
    
    func initialize() {
        lock.lock()
        let shouldInit = !isInitialized && !isInitializing
        if shouldInit {
            isInitializing = true
        }
        lock.unlock()
        
        guard shouldInit else {
            return // Already initialized or initializing
        }
        
        // Initialize asynchronously to avoid blocking the UI
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else { return }
            
            do {
                // Request camera permission (async)
                let status = AVCaptureDevice.authorizationStatus(for: .video)
                if status == .notDetermined {
                    // Request permission asynchronously
                    AVCaptureDevice.requestAccess(for: .video) { accessGranted in
                        if accessGranted {
                            self.setupCamera()
                        } else {
                            self.lock.lock()
                            self.isInitializing = false
                            self.lock.unlock()
                            print("[WebcamManager] Camera permission denied")
                        }
                    }
                    return // Will continue in callback
                } else if status != .authorized {
                    self.lock.lock()
                    self.isInitializing = false
                    self.lock.unlock()
                    print("[WebcamManager] Camera permission not granted")
                    return
                }
                
                // Permission already granted, set up camera
                self.setupCamera()
            }
        }
    }
    
    func enumerateCameras() -> [CameraInfo] {
        lock.lock()
        defer { lock.unlock() }
        
        var cameras: [CameraInfo] = []
        
        // Discover all available cameras
        let discoverySession = AVCaptureDevice.DiscoverySession(
            deviceTypes: [
                .builtInWideAngleCamera,
                .builtInUltraWideCamera,
                .builtInTelephotoCamera,
                .builtInTrueDepthCamera
            ],
            mediaType: .video,
            position: .unspecified
        )
        
        for device in discoverySession.devices {
            let position: String
            switch device.position {
            case .back:
                position = "back"
            case .front:
                position = "front"
            default:
                position = "unknown"
            }
            
            let deviceType: String
            if device.deviceType == .builtInWideAngleCamera {
                deviceType = "wide"
            } else if device.deviceType == .builtInUltraWideCamera {
                deviceType = "ultrawide"
            } else if device.deviceType == .builtInTelephotoCamera {
                deviceType = "telephoto"
            } else if device.deviceType == .builtInTrueDepthCamera {
                deviceType = "truedepth"
            } else {
                deviceType = "unknown"
            }
            
            let name = "\(position.capitalized) \(deviceType.capitalized)"
            cameras.append(CameraInfo(
                id: device.uniqueID,
                name: name,
                position: position,
                deviceType: deviceType
            ))
        }
        
        availableCameras = cameras
        return cameras
    }
    
    func getCameraCount() -> Int {
        lock.lock()
        defer { lock.unlock() }
        return availableCameras.count
    }
    
    func getCameraName(at index: Int) -> String? {
        lock.lock()
        defer { lock.unlock() }
        guard index >= 0 && index < availableCameras.count else {
            return nil
        }
        return availableCameras[index].name
    }
    
    func switchToCamera(at index: Int) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        
        guard index >= 0 && index < availableCameras.count else {
            return false
        }
        
        guard let session = captureSession, session.isRunning else {
            return false
        }
        
        let cameraInfo = availableCameras[index]
        
        // Find the device by unique ID
        let discoverySession = AVCaptureDevice.DiscoverySession(
            deviceTypes: [
                .builtInWideAngleCamera,
                .builtInUltraWideCamera,
                .builtInTelephotoCamera,
                .builtInTrueDepthCamera
            ],
            mediaType: .video,
            position: .unspecified
        )
        
        guard let device = discoverySession.devices.first(where: { $0.uniqueID == cameraInfo.id }) else {
            return false
        }
        
        // Remove old input
        if let oldInput = videoInput {
            session.removeInput(oldInput)
        }
        
        // Create new input
        do {
            let newInput = try AVCaptureDeviceInput(device: device)
            if session.canAddInput(newInput) {
                session.addInput(newInput)
                videoInput = newInput
                currentCameraIndex = index
                
                // Update video orientation
                if let connection = videoOutput?.connection(with: .video) {
                    connection.videoOrientation = .portrait
                }
                
                print("[WebcamManager] Switched to camera: \(cameraInfo.name)")
                return true
            }
        } catch {
            print("[WebcamManager] Failed to switch camera: \(error.localizedDescription)")
        }
        
        return false
    }
    
    func getCurrentCameraIndex() -> Int {
        lock.lock()
        defer { lock.unlock() }
        return currentCameraIndex
    }
    
    private func setupCamera() {
        do {
            // Enumerate cameras first
            _ = enumerateCameras()
            
            // Create capture session
            let session = AVCaptureSession()
            session.sessionPreset = .high // Use high quality preset
            
            // Get default video device (back camera) or first available
            let device: AVCaptureDevice?
            if availableCameras.isEmpty {
                device = AVCaptureDevice.default(.builtInWideAngleCamera, for: .video, position: .back) ??
                         AVCaptureDevice.default(for: .video)
            } else {
                // Use first camera from our list
                let discoverySession = AVCaptureDevice.DiscoverySession(
                    deviceTypes: [
                        .builtInWideAngleCamera,
                        .builtInUltraWideCamera,
                        .builtInTelephotoCamera,
                        .builtInTrueDepthCamera
                    ],
                    mediaType: .video,
                    position: .unspecified
                )
                device = discoverySession.devices.first(where: { $0.uniqueID == availableCameras[0].id })
            }
            
            guard let videoDevice = device else {
                throw NSError(domain: "WebcamManager", code: 2, userInfo: [NSLocalizedDescriptionKey: "No camera device found"])
            }
            
            // Create input
            let input: AVCaptureDeviceInput
            do {
                input = try AVCaptureDeviceInput(device: videoDevice)
            } catch {
                throw NSError(domain: "WebcamManager", code: 3, userInfo: [NSLocalizedDescriptionKey: "Failed to create camera input: \(error.localizedDescription)"])
            }
            
            guard session.canAddInput(input) else {
                throw NSError(domain: "WebcamManager", code: 4, userInfo: [NSLocalizedDescriptionKey: "Cannot add camera input to session"])
            }
            session.addInput(input)
            videoInput = input
            currentCameraIndex = 0
            
            // Create video output
            let output = AVCaptureVideoDataOutput()
            output.videoSettings = [
                kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA
            ]
            output.alwaysDiscardsLateVideoFrames = true
            
            // Set up queue for video output
            let queue = DispatchQueue(label: "com.xos.webcam.queue")
            output.setSampleBufferDelegate(self, queue: queue)
            
            guard session.canAddOutput(output) else {
                throw NSError(domain: "WebcamManager", code: 5, userInfo: [NSLocalizedDescriptionKey: "Cannot add video output to session"])
            }
            session.addOutput(output)
            
            // Set up video orientation connection - always use portrait for consistency
            // The camera app will handle rotation in the Rust code
            if let connection = output.connection(with: .video) {
                connection.videoOrientation = .portrait
            }
            
            // Store references
            self.lock.lock()
            self.captureSession = session
            self.videoOutput = output
            self.isInitialized = true
            self.isInitializing = false
            self.lock.unlock()
            
            // Start session
            session.startRunning()
            print("[WebcamManager] Camera initialized successfully")
        } catch {
            self.lock.lock()
            self.isInitializing = false
            self.lock.unlock()
            print("[WebcamManager] Camera setup error: \(error.localizedDescription)")
        }
    }
    
    func getResolution() -> (width: UInt32, height: UInt32) {
        lock.lock()
        defer { lock.unlock() }
        return latestResolution
    }
    
    func getLatestFrame(buffer: UnsafeMutablePointer<UInt8>, bufferSize: Int) -> Int {
        lock.lock()
        defer { lock.unlock() }
        
        guard let pixelBuffer = latestPixelBuffer else {
            return 0 // No frame available
        }
        
        let width = CVPixelBufferGetWidth(pixelBuffer)
        let height = CVPixelBufferGetHeight(pixelBuffer)
        let expectedSize = width * height * 3 // RGB format
        
        guard bufferSize >= expectedSize else {
            return 0 // Buffer too small
        }
        
        // Lock pixel buffer base address
        CVPixelBufferLockBaseAddress(pixelBuffer, .readOnly)
        defer {
            CVPixelBufferUnlockBaseAddress(pixelBuffer, .readOnly)
        }
        
        guard let baseAddress = CVPixelBufferGetBaseAddress(pixelBuffer) else {
            return 0
        }
        
        let bytesPerRow = CVPixelBufferGetBytesPerRow(pixelBuffer)
        let pixelFormat = CVPixelBufferGetPixelFormatType(pixelBuffer)
        
        // Convert BGRA to RGB
        if pixelFormat == kCVPixelFormatType_32BGRA {
            let src = baseAddress.assumingMemoryBound(to: UInt8.self)
            
            for y in 0..<height {
                for x in 0..<width {
                    let srcOffset = y * bytesPerRow + x * 4
                    let dstOffset = (y * width + x) * 3
                    
                    // BGRA -> RGB
                    buffer[dstOffset] = src[srcOffset + 2]     // R
                    buffer[dstOffset + 1] = src[srcOffset + 1] // G
                    buffer[dstOffset + 2] = src[srcOffset]     // B
                }
            }
            
            return expectedSize
        }
        
        return 0
    }
    
    func cleanup() {
        lock.lock()
        defer { lock.unlock() }
        
        if let session = captureSession, session.isRunning {
            session.stopRunning()
        }
        
        captureSession = nil
        videoOutput = nil
        latestPixelBuffer = nil
        latestResolution = (0, 0)
        isInitialized = false
    }
}

extension WebcamManager: AVCaptureVideoDataOutputSampleBufferDelegate {
    func captureOutput(_ output: AVCaptureOutput, didOutput sampleBuffer: CMSampleBuffer, from connection: AVCaptureConnection) {
        guard let pixelBuffer = CMSampleBufferGetImageBuffer(sampleBuffer) else {
            return
        }
        
        let width = UInt32(CVPixelBufferGetWidth(pixelBuffer))
        let height = UInt32(CVPixelBufferGetHeight(pixelBuffer))
        
        lock.lock()
        let wasEmpty = latestPixelBuffer == nil
        latestPixelBuffer = pixelBuffer
        latestResolution = (width, height)
        lock.unlock()
        
        // Log when first frame arrives
        if wasEmpty {
            print("[WebcamManager] First frame received: \(width)x\(height)")
        }
    }
}

// C-compatible FFI functions for Rust

@_cdecl("xos_webcam_init")
func xos_webcam_init() -> Int32 {
    WebcamManager.shared.initialize()
    return 0 // Always return success - initialization is async
}

@_cdecl("xos_webcam_get_resolution")
func xos_webcam_get_resolution(_ width: UnsafeMutablePointer<UInt32>?, _ height: UnsafeMutablePointer<UInt32>?) -> Int32 {
    let resolution = WebcamManager.shared.getResolution()
    
    if let width = width, let height = height {
        width.pointee = resolution.width
        height.pointee = resolution.height
    }
    
    return 0 // Success
}

@_cdecl("xos_webcam_get_frame")
func xos_webcam_get_frame(_ buffer: UnsafeMutablePointer<UInt8>?, _ bufferSize: Int) -> Int32 {
    guard let buffer = buffer else {
        return 0
    }
    
    let bytesWritten = WebcamManager.shared.getLatestFrame(buffer: buffer, bufferSize: bufferSize)
    return Int32(bytesWritten)
}

@_cdecl("xos_webcam_get_camera_count")
func xos_webcam_get_camera_count() -> Int32 {
    // Enumerate cameras if not already done
    _ = WebcamManager.shared.enumerateCameras()
    return Int32(WebcamManager.shared.getCameraCount())
}

@_cdecl("xos_webcam_get_camera_name")
func xos_webcam_get_camera_name(_ index: Int32, _ buffer: UnsafeMutablePointer<CChar>?, _ bufferSize: Int32) -> Int32 {
    guard let name = WebcamManager.shared.getCameraName(at: Int(index)) else {
        return 0
    }
    
    guard let buffer = buffer, bufferSize > 0 else {
        return 0
    }
    
    let nameData = name.data(using: .utf8) ?? Data()
    let bytesToCopy = min(Int(bufferSize) - 1, nameData.count)
    nameData.withUnsafeBytes { bytes in
        buffer.initialize(from: bytes.bindMemory(to: CChar.self).baseAddress!, count: bytesToCopy)
    }
    buffer[bytesToCopy] = 0 // Null terminate
    
    return Int32(bytesToCopy)
}

@_cdecl("xos_webcam_switch_camera")
func xos_webcam_switch_camera(_ index: Int32) -> Int32 {
    return WebcamManager.shared.switchToCamera(at: Int(index)) ? 0 : 1
}

@_cdecl("xos_webcam_get_current_camera_index")
func xos_webcam_get_current_camera_index() -> Int32 {
    return Int32(WebcamManager.shared.getCurrentCameraIndex())
}

