import UIKit
import Foundation

/// Protocol for console view controllers to receive log updates
public protocol ConsoleLogReceiver: AnyObject {
    func updateLogs(_ logs: [String])
}

/// Shared console manager for logging across the app
public class ConsoleManager {
    public static let shared = ConsoleManager()
    
    private var rawLogs: [(message: String, timestamp: Date)] = []
    private let maxLogs = 1000
    private weak var consoleReceiver: ConsoleLogReceiver?
    public var showTimestamps: Bool = false {
        didSet {
            // Update console view when timestamp setting changes
            consoleReceiver?.updateLogs(getFormattedLogs())
        }
    }
    
    private init() {}
    
    /// Add a log message to the console
    public func addLog(_ message: String) {
        let timestamp = Date()
        
        DispatchQueue.main.async { [weak self] in
            guard let self = self else { return }
            self.rawLogs.append((message: message, timestamp: timestamp))
            
            // Limit log count
            if self.rawLogs.count > self.maxLogs {
                self.rawLogs.removeFirst(self.rawLogs.count - self.maxLogs)
            }
            
            // Update console view if it exists
            self.consoleReceiver?.updateLogs(self.getFormattedLogs())
        }
    }
    
    /// Get formatted logs based on timestamp setting
    private func getFormattedLogs() -> [String] {
        if showTimestamps {
            let formatter = DateFormatter()
            formatter.dateStyle = .none
            formatter.timeStyle = .medium
            return rawLogs.map { log in
                let timestamp = formatter.string(from: log.timestamp)
                return "[\(timestamp)] \(log.message)"
            }
        } else {
            return rawLogs.map { $0.message }
        }
    }
    
    /// Register a console receiver to receive log updates
    public func registerConsole(_ receiver: ConsoleLogReceiver) {
        consoleReceiver = receiver
        receiver.updateLogs(getFormattedLogs())
    }
    
    /// Unregister console receiver
    public func unregisterConsole() {
        consoleReceiver = nil
    }
    
    /// Get all current logs (formatted based on timestamp setting)
    public func getLogs() -> [String] {
        return getFormattedLogs()
    }
    
    /// Clear all logs
    public func clearLogs() {
        rawLogs.removeAll()
        consoleReceiver?.updateLogs(getFormattedLogs())
    }
}

