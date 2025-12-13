import UIKit
import Xos
import Darwin

// Global signal handler storage
private var previousSignalHandlers: [Int32: sig_t] = [:]

// C-compatible signal handler function
private func signalHandler(_ signal: Int32) {
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
    DispatchQueue.main.async {
        NotificationCenter.default.post(
            name: NSNotification.Name("XosSwiftCrashed"),
            object: nil,
            userInfo: ["type": "Swift Crash", "message": errorMsg]
        )
    }
    
    // Restore previous handler and re-raise signal
    if let previous = previousSignalHandlers[signal] {
        Darwin.signal(signal, previous)
    }
    Darwin.raise(signal)
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
            ConsoleManager.shared.addLog("CRASH: [Objective-C Exception] \(errorMsg)")
            print("CRASH: [Objective-C Exception] \(errorMsg)")
            
            // Post notification
            DispatchQueue.main.async {
                NotificationCenter.default.post(
                    name: NSNotification.Name("XosSwiftCrashed"),
                    object: nil,
                    userInfo: ["type": "Objective-C Exception", "message": errorMsg]
                )
            }
        }
        
        // Set up signal handlers for Swift runtime crashes
        setupSignalHandlers()
        
        window = UIWindow(frame: UIScreen.main.bounds)
        
        let viewController = ViewController()
        window?.rootViewController = viewController
        window?.makeKeyAndVisible()
        
        ConsoleManager.shared.addLog("App launched")
        
        return true
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

