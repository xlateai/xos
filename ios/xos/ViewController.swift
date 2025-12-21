import UIKit
import Xos
import CoreHaptics

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
    
    // Haptic engine for chime feedback
    private var hapticEngine: CHHapticEngine?
    private var hapticPlayer: CHHapticAdvancedPatternPlayer?
    private var isPlayingHaptic: Bool = false
    
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
        
        // Setup haptic engine
        setupHapticEngine()
        
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
        
        // Disable fullscreen to show buttons and status bar
        isFullscreen = false
        
        // Show crash overlay in the viewport view (isolated application frame)
        viewportView.showCrashOverlay(crashType: "Rust crash", appName: crashedAppName)
        
        // Automatically open console after a brief delay
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
            self?.showConsole()
        }
    }
    
    @objc private func handleSwiftCrash(_ notification: Notification) {
        // Disable fullscreen to show buttons and status bar
        isFullscreen = false
        
        // Show crash overlay in the viewport view (isolated application frame)
        viewportView.showCrashOverlay(crashType: "Swift crash", appName: nil)
        
        // Automatically open console after a brief delay
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
            self?.showConsole()
        }
    }
    
    @objc private func changeApp() {
        let alert = UIAlertController(title: "Select App", message: nil, preferredStyle: .actionSheet)
        
        let apps = ["blank", "crash", "ball", "tracers", "camera", "whiteboard", "waveform", "scroll", "text", "wireframe", "triangles", "cursor", "audiovis", "audioedit", "partitions", "coder", "leds", "sensors"]
        
        for app in apps {
            alert.addAction(UIAlertAction(title: app, style: .default) { [weak self] _ in
                self?.appName = app
                self?.viewportView.hideCrashOverlay() // Hide crash overlay when changing apps
                // Reset to fullscreen when changing apps
                self?.isFullscreen = true
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
        // Allow simultaneous recognition so touches can pass through to viewport view
        panGesture.cancelsTouchesInView = false
        view.addGestureRecognizer(panGesture)
        
        let tapGesture = UITapGestureRecognizer(target: self, action: #selector(handleTapGesture(_:)))
        tapGesture.delegate = self
        // Only recognize tap if pan gesture is ready (swipe completed)
        tapGesture.cancelsTouchesInView = false
        view.addGestureRecognizer(tapGesture)
    }
    
    @objc private func handlePanGesture(_ gesture: UIPanGestureRecognizer) {
        // If gesture didn't start from left edge, ignore it (delegate already filtered)
        if !swipeActive && gesture.state != .began {
            return
        }
        
        let location = gesture.location(in: view)
        let screenWidth = view.bounds.width
        let screenHeight = view.bounds.height
        let leftEdgeThreshold = screenWidth * 0.15 // More lenient: 15% instead of 10%
        
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
                // Don't activate if not starting from left edge
                gestureReady = false
                swipeActive = false
            }
            
        case .changed:
            if swipeActive {
                let translation = gesture.translation(in: view)
                let currentX = gesture.location(in: view).x
                
                // More lenient: swipe 75% of screen width (instead of 85%) and current position > 80% (instead of 90%)
                if !gestureReady && translation.x > screenWidth * 0.75 && currentX > screenWidth * 0.8 {
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
        
        // Check if we have a completed swipe and tap happens within 375ms (1.5x longer tolerance)
        if gestureReady,
           let swipeTime = swipeCompleteTime,
           now.timeIntervalSince(swipeTime) < 0.375 {
            
            lastToggleTime = now
            isFullscreen.toggle()
            
            // Play haptic chime feedback
            playChimeHaptic()
            
            // Reset gesture state
            gestureReady = false
            swipeActive = false
            swipeCompleteTime = nil
        }
    }
    
    // MARK: - Haptic Feedback
    
    private func setupHapticEngine() {
        guard CHHapticEngine.capabilitiesForHardware().supportsHaptics else {
            return
        }
        
        do {
            hapticEngine = try CHHapticEngine()
            
            // Set up engine reset handler
            hapticEngine?.resetHandler = { [weak self] in
                guard let self = self else { return }
                do {
                    try self.hapticEngine?.start()
                } catch {
                    print("Failed to restart haptic engine: \(error)")
                }
            }
            
            try hapticEngine?.start()
        } catch {
            print("Failed to create haptic engine: \(error)")
        }
    }
    
    private func playChimeHaptic() {
        // Prevent overlapping haptics - if one is already playing, skip
        if isPlayingHaptic {
            return
        }
        
        guard let engine = hapticEngine,
              CHHapticEngine.capabilitiesForHardware().supportsHaptics else {
            // Fallback to simple impact feedback if CoreHaptics not available
            let generator = UIImpactFeedbackGenerator(style: .medium)
            generator.impactOccurred()
            return
        }
        
        // Stop any existing player
        if let player = hapticPlayer {
            do {
                try player.stop(atTime: 0)
            } catch {
                // Ignore stop errors
            }
            hapticPlayer = nil
        }
        
        // Create chime pattern matching sensorlab: fade in (0.0 -> 0.8) then fade out (0.8 -> 0.0)
        // Duration: 0.2s total, peak at 0.1s
        // Increased intensity for harder haptic feedback
        // Use a single continuous event with parameter curves for smooth transitions
        let intensityCurve = CHHapticParameterCurve(
            parameterID: .hapticIntensityControl,
            controlPoints: [
                CHHapticParameterCurve.ControlPoint(relativeTime: 0.0, value: 0.0),
                CHHapticParameterCurve.ControlPoint(relativeTime: 0.1, value: 0.8),
                CHHapticParameterCurve.ControlPoint(relativeTime: 0.2, value: 0.0)
            ],
            relativeTime: 0.0
        )
        
        let sharpnessCurve = CHHapticParameterCurve(
            parameterID: .hapticSharpnessControl,
            controlPoints: [
                CHHapticParameterCurve.ControlPoint(relativeTime: 0.0, value: 0.0),
                CHHapticParameterCurve.ControlPoint(relativeTime: 0.1, value: 0.6),
                CHHapticParameterCurve.ControlPoint(relativeTime: 0.2, value: 0.0)
            ],
            relativeTime: 0.0
        )
        
        let event = CHHapticEvent(
            eventType: .hapticContinuous,
            parameters: [
                CHHapticEventParameter(parameterID: .hapticIntensity, value: 0.8),
                CHHapticEventParameter(parameterID: .hapticSharpness, value: 0.6)
            ],
            relativeTime: 0.0,
            duration: 0.2
        )
        
        do {
            let pattern = try CHHapticPattern(events: [event], parameterCurves: [intensityCurve, sharpnessCurve])
            let player = try engine.makeAdvancedPlayer(with: pattern)
            
            isPlayingHaptic = true
            hapticPlayer = player
            try player.start(atTime: 0)
            
            // Reset flag after haptic duration (0.2 seconds) plus a small buffer
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) { [weak self] in
                self?.isPlayingHaptic = false
            }
        } catch {
            print("Failed to play haptic: \(error)")
            isPlayingHaptic = false
            // Fallback to simple impact
            let generator = UIImpactFeedbackGenerator(style: .medium)
            generator.impactOccurred()
        }
    }
}

// MARK: - UIGestureRecognizerDelegate

extension ViewController: UIGestureRecognizerDelegate {
    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith otherGestureRecognizer: UIGestureRecognizer) -> Bool {
        // Allow pan and tap gestures to work together
        return true
    }
    
    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        // Allow all touches - we'll filter in the handler
        // This ensures the gesture recognizer can track the gesture properly
        if gestureRecognizer is UIPanGestureRecognizer {
            return true
        }
        
        // For tap gesture, allow it but we'll check gestureReady in the handler
        if gestureRecognizer is UITapGestureRecognizer {
            return true
        }
        
        return true
    }
    
    func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
        // Allow gesture to begin - we'll check location in the handler
        // This ensures proper gesture tracking
        return true
    }
}

