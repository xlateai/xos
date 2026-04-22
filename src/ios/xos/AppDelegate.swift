import UIKit
import Xos
import Darwin

// Global signal handler storage
private var previousSignalHandlers: [Int32: sig_t] = [:]

// Global flag to prevent recursive signal handling
private var isHandlingCrash = false

// C-compatible signal handler function
private func signalHandler(_ signal: Int32) {
    // Prevent recursive calls
    guard !isHandlingCrash else {
        // If we're already handling a crash, restore default handler and abort
        if let previous = previousSignalHandlers[signal] {
            Darwin.signal(signal, previous)
        } else {
            Darwin.signal(signal, SIG_DFL)
        }
        Darwin.raise(signal)
        return
    }
    
    isHandlingCrash = true
    
    let signalName: String
    switch signal {
    case SIGABRT: signalName = "SIGABRT (Abort - fatalError/assertion)"
    case SIGILL: signalName = "SIGILL (Illegal Instruction)"
    case SIGSEGV: signalName = "SIGSEGV (Segmentation Violation)"
    case SIGBUS: signalName = "SIGBUS (Bus Error)"
    case SIGFPE: signalName = "SIGFPE (Floating Point Exception)"
    case SIGTRAP: signalName = "SIGTRAP (Trace Trap)"
    default: signalName = "Signal \(signal)"
    }
    
    // Get stack trace
    let stackTrace = Thread.callStackSymbols.prefix(20).joined(separator: "\n")
    let errorMsg = "Swift Runtime Crash: \(signalName)\nStack Trace:\n\(stackTrace)"
    
    // Log to console manager (synchronously to ensure it's logged)
    ConsoleManager.shared.addLog("CRASH: [Swift Crash] \(errorMsg)")
    print("CRASH: [Swift Crash] \(errorMsg)")
    
    // Post notification to show crash overlay
    // Use async dispatch to avoid blocking the main thread
    // This allows the UI to remain responsive even after a crash
    DispatchQueue.main.async {
        NotificationCenter.default.post(
            name: NSNotification.Name("XosSwiftCrashed"),
            object: nil,
            userInfo: ["type": "Swift Crash", "message": errorMsg, "signal": signal]
        )
    }
    
    // Don't immediately re-raise the signal - let the UI show the crash overlay
    // The app will stay alive so the user can see the crash message in the console
    // Only re-raise for truly fatal signals that we can't recover from
    // For now, we'll ignore the signal to keep the app alive
    // Note: This means the app might be in an unstable state, but at least the crash is visible
    
    // Set signal to be ignored so the app doesn't terminate immediately
    // This allows the crash overlay and console to be visible
    Darwin.signal(signal, SIG_IGN)
    
    // For truly fatal signals, we might want to exit after a delay
    // But for now, let's keep the app alive so the user can see the crash
    // The crash overlay will be visible and the console will show the error
}

@main
class AppDelegate: UIResponder, UIApplicationDelegate {
    var window: UIWindow?

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
    ) -> Bool {
        // Set up exception handler for Objective-C exceptions
        NSSetUncaughtExceptionHandler { exception in
            let errorMsg = "Uncaught Objective-C Exception: \(exception.name.rawValue)\nReason: \(exception.reason ?? "Unknown")\nStack: \(exception.callStackSymbols.prefix(10).joined(separator: "\n"))"
            ConsoleManager.shared.addLog("CRASH: [Swift Crash] \(errorMsg)")
            print("CRASH: [Swift Crash] \(errorMsg)")
            
            // Post notification (async to avoid blocking)
            DispatchQueue.main.async {
                NotificationCenter.default.post(
                    name: NSNotification.Name("XosSwiftCrashed"),
                    object: nil,
                    userInfo: ["type": "Swift Crash", "message": errorMsg]
                )
            }
        }
        
        // Set up signal handlers for Swift runtime crashes
        setupSignalHandlers()
        
        window = UIWindow(frame: UIScreen.main.bounds)
        
        let viewController = ViewController()
        window?.rootViewController = viewController
        window?.makeKeyAndVisible()
        
        // Set initial app name in console manager
        if let defaultApp = Bundle.main.infoDictionary?["XOSDefaultApp"] as? String,
           !defaultApp.isEmpty && defaultApp != "$(XOS_DEFAULT_APP)" {
            ConsoleManager.shared.setCurrentApp(defaultApp)
        } else {
            ConsoleManager.shared.setCurrentApp("blank")
        }
        
        ConsoleManager.shared.addLog("App launched")
        
        return true
    }
    
    func applicationDidBecomeActive(_ application: UIApplication) {
        // Ensures playAndRecord is active on first launch and after any OS interruption.
        XosForegroundAudio.reactivateSession()
    }
    
    private func setupSignalHandlers() {
        // Signals that indicate crashes
        let signals: [Int32] = [
            SIGABRT,  // Abort signal (fatalError, assertion failures)
            SIGILL,   // Illegal instruction
            SIGSEGV,  // Segmentation violation (bad memory access)
            SIGBUS,   // Bus error (bad memory access)
            SIGFPE,   // Floating point exception
            SIGTRAP   // Trace trap (breakpoint, etc.)
        ]
        
        for sigNum in signals {
            // Save previous handler
            let previous = Darwin.signal(sigNum, signalHandler)
            previousSignalHandlers[sigNum] = previous
        }
    }
}

