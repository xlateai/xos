import UIKit
import Metal
import MetalKit
import QuartzCore

/// High-performance Metal-based viewport renderer for XOS engine
public class XosViewportRenderer {
    private let device: MTLDevice
    private let commandQueue: MTLCommandQueue
    private var texture: MTLTexture?
    private var textureWidth: Int = 0
    private var textureHeight: Int = 0
    
    public init() {
        guard let device = MTLCreateSystemDefaultDevice() else {
            fatalError("Metal is not supported on this device")
        }
        self.device = device
        
        guard let queue = device.makeCommandQueue() else {
            fatalError("Failed to create Metal command queue")
        }
        self.commandQueue = queue
    }
    
    /// Update texture with pixel data (RGBA8 format)
    func updateTexture(width: Int, height: Int, rgbaData: UnsafePointer<UInt8>) {
        // Recreate texture if size changed
        if texture == nil || textureWidth != width || textureHeight != height {
            let textureDescriptor = MTLTextureDescriptor.texture2DDescriptor(
                pixelFormat: .rgba8Unorm,
                width: width,
                height: height,
                mipmapped: false
            )
            textureDescriptor.usage = [.shaderRead, .renderTarget]
            textureDescriptor.storageMode = .shared
            
            guard let newTexture = device.makeTexture(descriptor: textureDescriptor) else {
                print("ERROR: Failed to create Metal texture \(width)x\(height)")
                return
            }
            
            texture = newTexture
            textureWidth = width
            textureHeight = height
        }
        
        guard let tex = texture else { return }
        
        // Direct memory copy to texture
        let bytesPerRow = width * 4 // RGBA = 4 bytes per pixel
        let region = MTLRegion(
            origin: MTLOrigin(x: 0, y: 0, z: 0),
            size: MTLSize(width: width, height: height, depth: 1)
        )
        
        tex.replace(
            region: region,
            mipmapLevel: 0,
            withBytes: rgbaData,
            bytesPerRow: bytesPerRow
        )
    }
    
    /// Get the texture for rendering
    func getTexture() -> MTLTexture? {
        return texture
    }
    
    /// Get the Metal device
    func getDevice() -> MTLDevice {
        return device
    }
    
    /// Get the command queue
    func getCommandQueue() -> MTLCommandQueue {
        return commandQueue
    }
}

/// Native view component for XOS engine rendering
public class XosViewportView: UIView {
    private var metalLayer: CAMetalLayer?
    private let metalRenderer: XosViewportRenderer
    private var displayLink: CADisplayLink?
    private var appName: String = "blank"
    private var isEngineInitialized = false
    private var pendingAppName: String?
    private var crashOverlay: UIView?
    
    public required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
    
    public override init(frame: CGRect) {
        self.metalRenderer = XosViewportRenderer()
        super.init(frame: frame)
        clipsToBounds = true
        backgroundColor = UIColor.black
        
        // Create Metal layer for GPU rendering
        let metalLayer = CAMetalLayer()
        metalLayer.device = metalRenderer.getDevice()
        metalLayer.pixelFormat = .bgra8Unorm
        metalLayer.framebufferOnly = false
        metalLayer.contentsGravity = .resizeAspect
        self.layer.addSublayer(metalLayer)
        self.metalLayer = metalLayer
        
        // Set up Rust logging callback once
        setupRustLogging()
        
        // Listen for Swift crashes to show overlay in isolated frame
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleSwiftCrashNotification(_:)),
            name: NSNotification.Name("XosSwiftCrashed"),
            object: nil
        )
        
        // Engine will be initialized when setAppName is called and view has valid bounds
    }
    
    @objc private func handleSwiftCrashNotification(_ notification: Notification) {
        // Show crash overlay in this view (isolated application frame)
        showCrashOverlay(crashType: "Swift crash", appName: nil)
        stopAnimation()
    }
    
    private func initializeEngine() {
        // Don't initialize if bounds are invalid (zero width or height)
        let width = UInt32(bounds.width * UIScreen.main.scale)
        let height = UInt32(bounds.height * UIScreen.main.scale)
        
        guard width > 0 && height > 0 else {
            // View hasn't been laid out yet, will initialize in layoutSubviews
            return
        }
        
        // Don't initialize if already initialized with the same app
        guard !isEngineInitialized else {
            return
        }
        
        do {
            try xosEngineInit(appName: appName, width: width, height: height)
            isEngineInitialized = true
            pendingAppName = nil // Clear pending app name since we've initialized
            hasCrashed = false // Reset crash state on successful init
            hideCrashOverlay() // Hide any crash overlay on successful init
            ConsoleManager.shared.addLog("Engine initialized: \(appName) (\(width)x\(height))")
            startAnimation()
        } catch {
            let errorMsg = "Failed to initialize XOS engine: \(error.localizedDescription)"
            ConsoleManager.shared.addLog("ERROR: \(errorMsg)")
            print(errorMsg)
            // Show Swift crash overlay since this is a Swift-side error
            showCrashOverlay(crashType: "Swift crash", appName: nil)
            stopAnimation()
        }
    }
    
    private func setupRustLogging() {
        // Set up callback to receive Rust log messages
        let callback: @convention(c) (UnsafePointer<CChar>?) -> Void = { messagePtr in
            guard let messagePtr = messagePtr else { return }
            let message = String(cString: messagePtr)
            // Forward to console manager on main thread
            DispatchQueue.main.async {
                ConsoleManager.shared.addLog(message)
            }
        }
        xos_set_log_callback(callback)
    }
    
    public func setAppName(_ name: String) {
        appName = name
        hasCrashed = false // Reset crash state when changing apps
        hideCrashOverlay() // Hide crash overlay when changing apps
        
        // If engine is already initialized, cleanup and reinitialize
        if isEngineInitialized {
            ConsoleManager.shared.addLog("Changing app to: \(name)")
            stopAnimation()
            xosEngineCleanup()
            isEngineInitialized = false
        } else {
            // Engine not initialized yet, will initialize when view is laid out
            pendingAppName = name
        }
        
        // Try to initialize (will only work if bounds are valid)
        initializeEngine()
    }
    
    public override func layoutSubviews() {
        super.layoutSubviews()
        
        // Update Metal layer to match view size and screen scale
        if let metalLayer = metalLayer {
            let scale = UIScreen.main.scale
            metalLayer.frame = bounds
            metalLayer.drawableSize = CGSize(
                width: bounds.width * scale,
                height: bounds.height * scale
            )
            
            // If we have a pending app name and engine isn't initialized, initialize now
            if let pendingName = pendingAppName, !isEngineInitialized {
                appName = pendingName
                pendingAppName = nil
                initializeEngine()
            } else if isEngineInitialized {
                // Resize engine frame buffer if already initialized
                let width = UInt32(bounds.width * scale)
                let height = UInt32(bounds.height * scale)
                if !xosEngineResize(width: width, height: height) {
                    ConsoleManager.shared.addLog("WARNING: Engine resize failed (\(width)x\(height))")
                }
            }
        }
    }
    
    private func startAnimation() {
        stopAnimation()
        
        displayLink = CADisplayLink(target: self, selector: #selector(renderFrame))
        displayLink?.preferredFramesPerSecond = 60
        displayLink?.add(to: .main, forMode: .common)
    }
    
    private func stopAnimation() {
        displayLink?.invalidate()
        displayLink = nil
    }
    
    private var hasCrashed = false
    
    private func handleEngineCrash(_ message: String) {
        guard !hasCrashed else { return } // Only handle once
        hasCrashed = true
        isEngineInitialized = false // Reset initialization state
        
        stopAnimation()
        ConsoleManager.shared.addLog("ERROR: \(message)")
        
        // Show crash overlay in this view (isolated application frame)
        showCrashOverlay(crashType: "Rust crash", appName: appName)
        
        // Notify that engine has crashed (for console auto-open, etc.)
        NotificationCenter.default.post(
            name: NSNotification.Name("XosEngineCrashed"),
            object: nil,
            userInfo: ["appName": appName, "message": message]
        )
    }
    
    /// Show crash overlay in the isolated application frame
    public func showCrashOverlay(crashType: String, appName: String?) {
        // Remove existing overlay if any
        hideCrashOverlay()
        
        // Create crash overlay
        let overlay = UIView()
        overlay.backgroundColor = .black
        overlay.translatesAutoresizingMaskIntoConstraints = false
        addSubview(overlay)
        
        // Create crash message label
        let label = UILabel()
        if let appName = appName {
            label.text = "\(crashType)\n\(appName)"
        } else {
            label.text = crashType
        }
        label.textColor = .white
        label.font = UIFont.systemFont(ofSize: 24, weight: .medium)
        label.textAlignment = .center
        label.numberOfLines = 0
        label.translatesAutoresizingMaskIntoConstraints = false
        overlay.addSubview(label)
        
        NSLayoutConstraint.activate([
            overlay.topAnchor.constraint(equalTo: topAnchor),
            overlay.leadingAnchor.constraint(equalTo: leadingAnchor),
            overlay.trailingAnchor.constraint(equalTo: trailingAnchor),
            overlay.bottomAnchor.constraint(equalTo: bottomAnchor),
            
            label.centerXAnchor.constraint(equalTo: overlay.centerXAnchor),
            label.centerYAnchor.constraint(equalTo: overlay.centerYAnchor)
        ])
        
        crashOverlay = overlay
    }
    
    /// Hide crash overlay
    public func hideCrashOverlay() {
        crashOverlay?.removeFromSuperview()
        crashOverlay = nil
    }
    
    @objc private func renderFrame() {
        // Wrap in do-catch to catch any Swift exceptions
        do {
            // Tick the engine
            guard xosEngineTick() else {
                // Engine tick failed - this might indicate a crash
                handleEngineCrash("Engine tick failed - engine may have crashed")
                return
            }
            
            // Get frame buffer from engine
            guard let frameBuffer = xosEngineGetFrameBuffer(),
                  let size = xosEngineGetFrameSize() else {
                // Frame buffer unavailable - might be a crash
                handleEngineCrash("Failed to get frame buffer - engine may have crashed")
                return
            }
            
            let width = Int(size.width)
            let height = Int(size.height)
            
            // Update Metal texture
            metalRenderer.updateTexture(
                width: width,
                height: height,
                rgbaData: frameBuffer
            )
            
            // Render using Metal
            guard let metalLayer = metalLayer,
                  let drawable = metalLayer.nextDrawable(),
                  let sourceTexture = metalRenderer.getTexture() else {
                return
            }
            
            // Use blit encoder for fast texture copy
            guard let commandBuffer = metalRenderer.getCommandQueue().makeCommandBuffer(),
                  let blitEncoder = commandBuffer.makeBlitCommandEncoder() else {
                return
            }
            
            // Copy source texture to drawable
            blitEncoder.copy(
                from: sourceTexture,
                sourceSlice: 0,
                sourceLevel: 0,
                sourceOrigin: MTLOrigin(x: 0, y: 0, z: 0),
                sourceSize: MTLSize(width: width, height: height, depth: 1),
                to: drawable.texture,
                destinationSlice: 0,
                destinationLevel: 0,
                destinationOrigin: MTLOrigin(x: 0, y: 0, z: 0)
            )
            
            blitEncoder.endEncoding()
            commandBuffer.present(drawable)
            commandBuffer.commit()
        } catch {
            // Catch any Swift exceptions during rendering
            let errorMsg = "Swift exception during rendering: \(error.localizedDescription)"
            ConsoleManager.shared.addLog("CRASH: [Swift Crash] \(errorMsg)")
            showCrashOverlay(crashType: "Swift crash", appName: nil)
            stopAnimation()
        }
    }
    
    public override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        super.touchesBegan(touches, with: event)
        if let touch = touches.first {
            let location = touch.location(in: self)
            let scale = UIScreen.main.scale
            if !xosEngineUpdateMouse(x: Float(location.x * scale), y: Float(location.y * scale)) {
                ConsoleManager.shared.addLog("WARNING: Failed to update mouse position")
            }
            if !xosEngineMouseDown() {
                ConsoleManager.shared.addLog("WARNING: Failed to handle mouse down")
            }
        }
    }
    
    public override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        super.touchesMoved(touches, with: event)
        if let touch = touches.first {
            let location = touch.location(in: self)
            let scale = UIScreen.main.scale
            if !xosEngineUpdateMouse(x: Float(location.x * scale), y: Float(location.y * scale)) {
                ConsoleManager.shared.addLog("WARNING: Failed to update mouse position")
            }
        }
    }
    
    public override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        super.touchesEnded(touches, with: event)
        if !xosEngineMouseUp() {
            ConsoleManager.shared.addLog("WARNING: Failed to handle mouse up")
        }
    }
    
    public override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        super.touchesCancelled(touches, with: event)
        if !xosEngineMouseUp() {
            ConsoleManager.shared.addLog("WARNING: Failed to handle mouse up")
        }
    }
    
    deinit {
        NotificationCenter.default.removeObserver(self)
        stopAnimation()
        xosEngineCleanup()
    }
}

