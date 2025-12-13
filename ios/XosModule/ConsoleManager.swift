import UIKit
import Foundation

/// Protocol for console view controllers to receive log updates
public protocol ConsoleLogReceiver: AnyObject {
    func updateLogs(_ logs: [String])
}

/// Shared console manager for logging across the app
public class ConsoleManager {
    public static let shared = ConsoleManager()
    
    private var logs: [String] = []
    private let maxLogs = 1000
    private weak var consoleReceiver: ConsoleLogReceiver?
    
    private init() {}
    
    /// Add a log message to the console
    public func addLog(_ message: String) {
        let timestamp = DateFormatter.localizedString(from: Date(), dateStyle: .none, timeStyle: .medium)
        let logEntry = "[\(timestamp)] \(message)"
        
        DispatchQueue.main.async { [weak self] in
            guard let self = self else { return }
            self.logs.append(logEntry)
            
            // Limit log count
            if self.logs.count > self.maxLogs {
                self.logs.removeFirst(self.logs.count - self.maxLogs)
            }
            
            // Update console view if it exists
            self.consoleReceiver?.updateLogs(self.logs)
        }
    }
    
    /// Register a console receiver to receive log updates
    public func registerConsole(_ receiver: ConsoleLogReceiver) {
        consoleReceiver = receiver
        receiver.updateLogs(logs)
    }
    
    /// Unregister console receiver
    public func unregisterConsole() {
        consoleReceiver = nil
    }
    
    /// Get all current logs
    public func getLogs() -> [String] {
        return logs
    }
    
    /// Clear all logs
    public func clearLogs() {
        logs.removeAll()
        consoleReceiver?.updateLogs(logs)
    }
}

