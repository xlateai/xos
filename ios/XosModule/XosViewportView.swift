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
        let width = UInt32(bounds.width * UIScreen.main.scale)
        let height = UInt32(bounds.height * UIScreen.main.scale)
        
        do {
            try xosEngineInit(appName: appName, width: width, height: height)
            startAnimation()
        } catch {
            print("Failed to initialize XOS engine: \(error)")
        }
    }
    
    public func setAppName(_ name: String) {
        appName = name
        xosEngineCleanup()
        initializeEngine()
    }
    
    override func layoutSubviews() {
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
            _ = xosEngineResize(width: width, height: height)
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
    
    @objc private func renderFrame() {
        // Tick the engine
        guard xosEngineTick() else {
            return
        }
        
        // Get frame buffer from engine
        guard let frameBuffer = xosEngineGetFrameBuffer(),
              let size = xosEngineGetFrameSize() else {
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
    
    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        super.touchesBegan(touches, with: event)
        if let touch = touches.first {
            let location = touch.location(in: self)
            let scale = UIScreen.main.scale
            _ = xosEngineUpdateMouse(x: Float(location.x * scale), y: Float(location.y * scale))
            _ = xosEngineMouseDown()
        }
    }
    
    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        super.touchesMoved(touches, with: event)
        if let touch = touches.first {
            let location = touch.location(in: self)
            let scale = UIScreen.main.scale
            _ = xosEngineUpdateMouse(x: Float(location.x * scale), y: Float(location.y * scale))
        }
    }
    
    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        super.touchesEnded(touches, with: event)
        _ = xosEngineMouseUp()
    }
    
    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        super.touchesCancelled(touches, with: event)
        _ = xosEngineMouseUp()
    }
    
    deinit {
        stopAnimation()
        xosEngineCleanup()
    }
}

