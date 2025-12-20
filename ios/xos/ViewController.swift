import UIKit
import Xos

class ViewController: UIViewController {
    private var viewportView: XosViewportView!
    private var changeAppButton: UIButton!
    private var consoleButton: UIButton!
    private var appName: String = {
        // Try to get app name from Info.plist (set during build)
        if let defaultApp = Bundle.main.infoDictionary?["XOSDefaultApp"] as? String,
           !defaultApp.isEmpty && defaultApp != "$(XOS_DEFAULT_APP)" {
            return defaultApp
        }
        // Fallback to blank
        return "blank"
    }()
    
    // Fullscreen state
    private var isFullscreen: Bool = true {
        didSet {
            updateFullscreenState()
        }
    }
    
    // Gesture tracking
    private var gestureReady: Bool = false
    private var swipeActive: Bool = false
    private var swipeCompleteTime: Date?
    private var lastToggleTime: Date = Date()
    
    override func viewDidLoad() {
        super.viewDidLoad()
        
        view.backgroundColor = .black
        
        // Create viewport view
        viewportView = XosViewportView(frame: view.bounds)
        viewportView.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        viewportView.setAppName(appName)
        view.addSubview(viewportView)
        
        // Add app selector button (optional, for testing different apps)
        changeAppButton = UIButton(type: .system)
        changeAppButton.setTitle("Change App", for: .normal)
        changeAppButton.addTarget(self, action: #selector(changeApp), for: .touchUpInside)
        changeAppButton.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(changeAppButton)
        
        // Add console button
        consoleButton = UIButton(type: .system)
        consoleButton.setTitle("Console", for: .normal)
        consoleButton.addTarget(self, action: #selector(showConsole), for: .touchUpInside)
        consoleButton.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(consoleButton)
        
        NSLayoutConstraint.activate([
            changeAppButton.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 20),
            changeAppButton.trailingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.trailingAnchor, constant: -20),
            
            consoleButton.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 20),
            consoleButton.trailingAnchor.constraint(equalTo: changeAppButton.leadingAnchor, constant: -10)
        ])
        
        // Set initial fullscreen state
        updateFullscreenState()
        
        // Setup gesture recognizer for swipe-left-to-right + tap
        setupGestureRecognizer()
        
        // Listen for engine crashes
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleEngineCrash(_:)),
            name: NSNotification.Name("XosEngineCrashed"),
            object: nil
        )
        
        // Listen for Swift crashes
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleSwiftCrash(_:)),
            name: NSNotification.Name("XosSwiftCrashed"),
            object: nil
        )
    }
    
    deinit {
        NotificationCenter.default.removeObserver(self)
    }
    
    @objc private func handleEngineCrash(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let crashedAppName = userInfo["appName"] as? String else {
            return
        }
        
        // Show crash overlay in the viewport view (isolated application frame)
        viewportView.showCrashOverlay(crashType: "Rust crash", appName: crashedAppName)
        
        // Automatically open console after a brief delay
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
            self?.showConsole()
        }
    }
    
    @objc private func handleSwiftCrash(_ notification: Notification) {
        // Show crash overlay in the viewport view (isolated application frame)
        viewportView.showCrashOverlay(crashType: "Swift crash", appName: nil)
        
        // Automatically open console after a brief delay
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
            self?.showConsole()
        }
    }
    
    @objc private func changeApp() {
        let alert = UIAlertController(title: "Select App", message: nil, preferredStyle: .actionSheet)
        
        let apps = ["blank", "crash", "ball", "tracers", "camera", "whiteboard", "waveform", "scroll", "text", "wireframe", "triangles", "cursor", "audiovis", "audioedit", "partitions", "coder", "leds"]
        
        for app in apps {
            alert.addAction(UIAlertAction(title: app, style: .default) { [weak self] _ in
                self?.appName = app
                self?.viewportView.hideCrashOverlay() // Hide crash overlay when changing apps
                self?.viewportView.setAppName(app)
            })
        }
        
        alert.addAction(UIAlertAction(title: "Cancel", style: .cancel))
        
        present(alert, animated: true)
    }
    
    @objc private func showConsole() {
        let consoleVC = ConsoleViewController()
        consoleVC.modalPresentationStyle = UIModalPresentationStyle.fullScreen
        present(consoleVC, animated: true)
    }
    
    // MARK: - Fullscreen Management
    
    private func updateFullscreenState() {
        // Show/hide buttons
        changeAppButton.isHidden = isFullscreen
        consoleButton.isHidden = isFullscreen
        
        // Update status bar
        setNeedsStatusBarAppearanceUpdate()
    }
    
    override var prefersStatusBarHidden: Bool {
        return isFullscreen
    }
    
    override var preferredStatusBarUpdateAnimation: UIStatusBarAnimation {
        return .fade
    }
    
    // MARK: - Gesture Recognizer
    
    private func setupGestureRecognizer() {
        let panGesture = UIPanGestureRecognizer(target: self, action: #selector(handlePanGesture(_:)))
        panGesture.delegate = self
        view.addGestureRecognizer(panGesture)
        
        let tapGesture = UITapGestureRecognizer(target: self, action: #selector(handleTapGesture(_:)))
        tapGesture.delegate = self
        view.addGestureRecognizer(tapGesture)
    }
    
    @objc private func handlePanGesture(_ gesture: UIPanGestureRecognizer) {
        let location = gesture.location(in: view)
        let screenWidth = view.bounds.width
        let screenHeight = view.bounds.height
        let leftEdgeThreshold = screenWidth * 0.1
        
        switch gesture.state {
        case .began:
            // Only start gesture if on left edge and not in bottom 10% (button area)
            let isLeftEdge = location.x < leftEdgeThreshold
            let isBottomArea = location.y > screenHeight * 0.9
            
            if isLeftEdge && !isBottomArea {
                gestureReady = false
                swipeActive = true
                swipeCompleteTime = nil
            } else {
                gestureReady = false
                swipeActive = false
            }
            
        case .changed:
            if swipeActive {
                let translation = gesture.translation(in: view)
                let currentX = gesture.location(in: view).x
                
                // Check if swipe has moved far enough right (85% of screen width)
                if !gestureReady && translation.x > screenWidth * 0.85 && currentX > screenWidth * 0.9 {
                    gestureReady = true
                    swipeCompleteTime = Date()
                }
            }
            
        case .ended, .cancelled, .failed:
            swipeActive = false
            gestureReady = false
            swipeCompleteTime = nil
            
        default:
            break
        }
    }
    
    @objc private func handleTapGesture(_ gesture: UITapGestureRecognizer) {
        let now = Date()
        
        // Prevent rapid toggling (debounce)
        if now.timeIntervalSince(lastToggleTime) < 0.3 {
            return
        }
        
        // Check if we have a completed swipe and tap happens within 250ms
        if gestureReady,
           let swipeTime = swipeCompleteTime,
           now.timeIntervalSince(swipeTime) < 0.25 {
            
            lastToggleTime = now
            isFullscreen.toggle()
            
            // Reset gesture state
            gestureReady = false
            swipeActive = false
            swipeCompleteTime = nil
        }
    }
}

// MARK: - UIGestureRecognizerDelegate

extension ViewController: UIGestureRecognizerDelegate {
    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith otherGestureRecognizer: UIGestureRecognizer) -> Bool {
        // Allow pan and tap gestures to work together
        return true
    }
}

