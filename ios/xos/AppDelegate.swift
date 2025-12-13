import UIKit
import Xos

@main
class AppDelegate: UIResponder, UIApplicationDelegate {
    var window: UIWindow?

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
    ) -> Bool {
        // Set up exception handler for Objective-C exceptions
        NSSetUncaughtExceptionHandler { exception in
            let errorMsg = "Uncaught Exception: \(exception.name.rawValue)\nReason: \(exception.reason ?? "Unknown")\nStack: \(exception.callStackSymbols.prefix(10).joined(separator: "\n"))"
            ConsoleManager.shared.addLog("CRASH: \(errorMsg)")
            print(errorMsg)
        }
        
        window = UIWindow(frame: UIScreen.main.bounds)
        
        let viewController = ViewController()
        window?.rootViewController = viewController
        window?.makeKeyAndVisible()
        
        ConsoleManager.shared.addLog("App launched")
        
        return true
    }
}

