import UIKit
import Foundation

/// Protocol for console view controllers to receive log updates
public protocol ConsoleLogReceiver: AnyObject {
    func updateLogs(_ sections: [LogSection])
}

/// Represents a collapsible log section for a specific app
public struct LogSection {
    public let appName: String
    public let logs: [String]
    public var isCollapsed: Bool
    
    public init(appName: String, logs: [String], isCollapsed: Bool = true) {
        self.appName = appName
        self.logs = logs
        self.isCollapsed = isCollapsed
    }
}

/// Shared console manager for logging across the app
public class ConsoleManager {
    public static let shared = ConsoleManager()
    
    private var currentAppName: String = "unknown"
    private var rawLogs: [(message: String, timestamp: Date)] = []
    private var archivedSections: [LogSection] = [] // Archived logs by app name
    private let maxLogs = 1000
    private weak var consoleReceiver: ConsoleLogReceiver?
    public var showTimestamps: Bool = false {
        didSet {
            // Update console view when timestamp setting changes
            consoleReceiver?.updateLogs(getAllSections())
        }
    }
    
    private init() {
        // Set initial app name
        currentAppName = "unknown"
    }
    
    /// Set the current app name (called when app changes)
    public func setCurrentApp(_ appName: String) {
        // Archive current logs if we have any
        if !rawLogs.isEmpty {
            let formattedLogs = getFormattedLogs(from: rawLogs)
            archivedSections.append(LogSection(appName: currentAppName, logs: formattedLogs, isCollapsed: true))
            
            // Limit archived sections
            if archivedSections.count > 50 {
                archivedSections.removeFirst()
            }
        }
        
        // Clear current logs and set new app name
        rawLogs.removeAll()
        currentAppName = appName
        
        // Notify receiver
        consoleReceiver?.updateLogs(getAllSections())
    }
    
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
            self.consoleReceiver?.updateLogs(self.getAllSections())
        }
    }
    
    /// Get formatted logs from a specific log array
    private func getFormattedLogs(from logs: [(message: String, timestamp: Date)]) -> [String] {
        if showTimestamps {
            let formatter = DateFormatter()
            formatter.dateStyle = .none
            formatter.timeStyle = .medium
            return logs.map { log in
                let timestamp = formatter.string(from: log.timestamp)
                return "[\(timestamp)] \(log.message)"
            }
        } else {
            return logs.map { $0.message }
        }
    }
    
    /// Get all sections (archived + current)
    private func getAllSections() -> [LogSection] {
        var sections = archivedSections
        
        // Add current app logs as a non-collapsed section
        if !rawLogs.isEmpty {
            let currentLogs = getFormattedLogs(from: rawLogs)
            sections.append(LogSection(appName: currentAppName, logs: currentLogs, isCollapsed: false))
        }
        
        return sections
    }
    
    /// Register a console receiver to receive log updates
    public func registerConsole(_ receiver: ConsoleLogReceiver) {
        consoleReceiver = receiver
        receiver.updateLogs(getAllSections())
    }
    
    /// Unregister console receiver
    public func unregisterConsole() {
        consoleReceiver = nil
    }
    
    /// Get all sections (for external access)
    public func getSections() -> [LogSection] {
        return getAllSections()
    }
    
    /// Clear all logs (both current and archived)
    public func clearLogs() {
        rawLogs.removeAll()
        archivedSections.removeAll()
        consoleReceiver?.updateLogs(getAllSections())
    }
    
    /// Toggle collapse state of a section
    public func toggleSection(at index: Int) {
        let allSections = getAllSections()
        guard index < allSections.count else { return }
        
        // If it's an archived section, toggle it
        if index < archivedSections.count {
            archivedSections[index].isCollapsed.toggle()
        }
        // Note: Current section (last one) is always expanded, so we don't toggle it
        
        consoleReceiver?.updateLogs(getAllSections())
    }
}

