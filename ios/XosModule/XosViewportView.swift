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
        
        // Initialize engine when view is ready
        DispatchQueue.main.async { [weak self] in
            self?.initializeEngine()
        }
    }
    
    private func initializeEngine() {
        // Set up Rust logging callback before initializing engine
        setupRustLogging()
        
        let width = UInt32(bounds.width * UIScreen.main.scale)
        let height = UInt32(bounds.height * UIScreen.main.scale)
        
        do {
            try xosEngineInit(appName: appName, width: width, height: height)
            hasCrashed = false // Reset crash state on successful init
            ConsoleManager.shared.addLog("Engine initialized: \(appName) (\(width)x\(height))")
            startAnimation()
        } catch {
            let errorMsg = "Failed to initialize XOS engine: \(error.localizedDescription)"
            ConsoleManager.shared.addLog("ERROR: \(errorMsg)")
            print(errorMsg)
            handleEngineCrash(errorMsg)
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
        ConsoleManager.shared.addLog("Changing app to: \(name)")
        xosEngineCleanup()
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
            
            // Resize engine frame buffer
            let width = UInt32(bounds.width * scale)
            let height = UInt32(bounds.height * scale)
            if !xosEngineResize(width: width, height: height) {
                ConsoleManager.shared.addLog("WARNING: Engine resize failed (\(width)x\(height))")
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
        
        stopAnimation()
        ConsoleManager.shared.addLog("ERROR: \(message)")
        
        // Notify that engine has crashed
        NotificationCenter.default.post(
            name: NSNotification.Name("XosEngineCrashed"),
            object: nil,
            userInfo: ["appName": appName, "message": message]
        )
    }
    
    @objc private func renderFrame() {
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
        stopAnimation()
        xosEngineCleanup()
    }
}

