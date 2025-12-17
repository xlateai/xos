import Foundation
import AVFoundation
import CoreVideo

// Shared camera manager
final class WebcamManager: NSObject {
    static let shared = WebcamManager()
    
    private var captureSession: AVCaptureSession?
    private var videoOutput: AVCaptureVideoDataOutput?
    private var latestPixelBuffer: CVPixelBuffer?
    private var latestResolution: (width: UInt32, height: UInt32) = (0, 0)
    private let lock = NSLock()
    private var isInitialized = false
    private var isInitializing = false
    
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
    
    private func setupCamera() {
        do {
            // Create capture session
            let session = AVCaptureSession()
            session.sessionPreset = .high // Use high quality preset
            
            // Get default video device (back camera)
            guard let videoDevice = AVCaptureDevice.default(.builtInWideAngleCamera, for: .video, position: .back) ??
                                    AVCaptureDevice.default(for: .video) else {
                throw NSError(domain: "WebcamManager", code: 2, userInfo: [NSLocalizedDescriptionKey: "No camera device found"])
            }
            
            // Create input
            let videoInput: AVCaptureDeviceInput
            do {
                videoInput = try AVCaptureDeviceInput(device: videoDevice)
            } catch {
                throw NSError(domain: "WebcamManager", code: 3, userInfo: [NSLocalizedDescriptionKey: "Failed to create camera input: \(error.localizedDescription)"])
            }
            
            guard session.canAddInput(videoInput) else {
                throw NSError(domain: "WebcamManager", code: 4, userInfo: [NSLocalizedDescriptionKey: "Cannot add camera input to session"])
            }
            session.addInput(videoInput)
            
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

