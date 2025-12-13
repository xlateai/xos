import UIKit
import Xos

class ConsoleViewController: UIViewController, UIScrollViewDelegate, ConsoleLogReceiver {
    private var scrollView: UIScrollView!
    private var contentView: UIStackView!
    private var sections: [LogSection] = []
    private var sectionViews: [UIView] = []
    private var isScrolledToBottom = true
    private var timestampButton: UIButton!
    
    override func viewDidLoad() {
        super.viewDidLoad()
        
        view.backgroundColor = UIColor(white: 0.1, alpha: 1.0)
        
        // Create scroll view
        scrollView = UIScrollView()
        scrollView.translatesAutoresizingMaskIntoConstraints = false
        scrollView.backgroundColor = UIColor(white: 0.05, alpha: 1.0)
        scrollView.delegate = self
        view.addSubview(scrollView)
        
        // Create content view (vertical stack)
        contentView = UIStackView()
        contentView.axis = .vertical
        contentView.spacing = 0
        contentView.translatesAutoresizingMaskIntoConstraints = false
        scrollView.addSubview(contentView)
        
        // Add close button
        let closeButton = UIButton(type: .system)
        closeButton.setTitle("✕", for: .normal)
        closeButton.titleLabel?.font = UIFont.systemFont(ofSize: 24, weight: .bold)
        closeButton.setTitleColor(.white, for: .normal)
        closeButton.backgroundColor = UIColor(white: 0.2, alpha: 0.8)
        closeButton.layer.cornerRadius = 20
        closeButton.translatesAutoresizingMaskIntoConstraints = false
        closeButton.addTarget(self, action: #selector(closeTapped), for: .touchUpInside)
        view.addSubview(closeButton)
        
        // Add clear button
        let clearButton = UIButton(type: .system)
        clearButton.setTitle("Clear", for: .normal)
        clearButton.setTitleColor(.white, for: .normal)
        clearButton.backgroundColor = UIColor(white: 0.2, alpha: 0.8)
        clearButton.layer.cornerRadius = 8
        clearButton.translatesAutoresizingMaskIntoConstraints = false
        clearButton.addTarget(self, action: #selector(clearTapped), for: .touchUpInside)
        view.addSubview(clearButton)
        
        // Add timestamp toggle button
        timestampButton = UIButton(type: .system)
        timestampButton.setTitle("Show Timestamps", for: .normal)
        timestampButton.setTitleColor(.white, for: .normal)
        timestampButton.backgroundColor = UIColor(white: 0.2, alpha: 0.8)
        timestampButton.layer.cornerRadius = 8
        timestampButton.translatesAutoresizingMaskIntoConstraints = false
        timestampButton.addTarget(self, action: #selector(timestampTapped), for: .touchUpInside)
        updateTimestampButton(timestampButton)
        view.addSubview(timestampButton)
        
        NSLayoutConstraint.activate([
            scrollView.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 50),
            scrollView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            scrollView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            scrollView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            
            contentView.topAnchor.constraint(equalTo: scrollView.topAnchor),
            contentView.leadingAnchor.constraint(equalTo: scrollView.leadingAnchor),
            contentView.trailingAnchor.constraint(equalTo: scrollView.trailingAnchor),
            contentView.bottomAnchor.constraint(equalTo: scrollView.bottomAnchor),
            contentView.widthAnchor.constraint(equalTo: scrollView.widthAnchor),
            
            closeButton.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 10),
            closeButton.trailingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.trailingAnchor, constant: -10),
            closeButton.widthAnchor.constraint(equalToConstant: 40),
            closeButton.heightAnchor.constraint(equalToConstant: 40),
            
            clearButton.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 10),
            clearButton.leadingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.leadingAnchor, constant: 10),
            clearButton.heightAnchor.constraint(equalToConstant: 40),
            
            timestampButton.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 10),
            timestampButton.leadingAnchor.constraint(equalTo: clearButton.trailingAnchor, constant: 10),
            timestampButton.heightAnchor.constraint(equalToConstant: 40),
        ])
        
        // Register with console manager
        ConsoleManager.shared.registerConsole(self)
        sections = ConsoleManager.shared.getSections()
        updateSections()
    }
    
    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        ConsoleManager.shared.unregisterConsole()
    }
    
    // MARK: - UIScrollViewDelegate
    
    func scrollViewDidScroll(_ scrollView: UIScrollView) {
        let bottom = scrollView.contentSize.height - scrollView.bounds.height
        isScrolledToBottom = scrollView.contentOffset.y >= bottom - 10
    }
    
    @objc private func closeTapped() {
        dismiss(animated: true)
    }
    
    @objc private func clearTapped() {
        ConsoleManager.shared.clearLogs()
    }
    
    @objc private func timestampTapped(_ sender: UIButton) {
        ConsoleManager.shared.showTimestamps.toggle()
        updateTimestampButton(sender)
    }
    
    private func updateTimestampButton(_ button: UIButton) {
        let showTimestamps = ConsoleManager.shared.showTimestamps
        button.setTitle(showTimestamps ? "Hide Timestamps" : "Show Timestamps", for: .normal)
    }
    
    /// Update logs from ConsoleManager
    func updateLogs(_ newSections: [LogSection]) {
        sections = newSections
        updateSections()
    }
    
    private func updateSections() {
        // Remove all existing section views
        sectionViews.forEach { $0.removeFromSuperview() }
        sectionViews.removeAll()
        
        // Create views for each section
        for (index, section) in sections.enumerated() {
            let sectionView = createSectionView(section: section, index: index)
            contentView.addArrangedSubview(sectionView)
            sectionViews.append(sectionView)
        }
        
        // Auto-scroll to bottom if user was already at bottom
        if isScrolledToBottom {
            DispatchQueue.main.async { [weak self] in
                guard let self = self else { return }
                let bottom = self.scrollView.contentSize.height - self.scrollView.bounds.height
                if bottom > 0 {
                    self.scrollView.setContentOffset(CGPoint(x: 0, y: bottom), animated: false)
                }
            }
        }
    }
    
    private func createSectionView(section: LogSection, index: Int) -> UIView {
        let containerView = UIView()
        containerView.backgroundColor = UIColor(white: 0.08, alpha: 1.0)
        containerView.layer.borderWidth = 1
        containerView.layer.borderColor = UIColor(white: 0.2, alpha: 1.0).cgColor
        
        // Header button (collapsible)
        let headerButton = UIButton(type: .system)
        headerButton.translatesAutoresizingMaskIntoConstraints = false
        headerButton.backgroundColor = UIColor(white: 0.15, alpha: 1.0)
        headerButton.setTitle("\(section.isCollapsed ? "▶" : "▼") \(section.appName) (\(section.logs.count) logs)", for: .normal)
        headerButton.setTitleColor(.white, for: .normal)
        headerButton.titleLabel?.font = UIFont.systemFont(ofSize: 14, weight: .semibold)
        headerButton.contentHorizontalAlignment = .left
        headerButton.contentEdgeInsets = UIEdgeInsets(top: 0, left: 12, bottom: 0, right: 12)
        headerButton.tag = index
        headerButton.addTarget(self, action: #selector(sectionHeaderTapped(_:)), for: .touchUpInside)
        containerView.addSubview(headerButton)
        
        // Logs text view (collapsible content)
        let logsTextView = UITextView()
        logsTextView.translatesAutoresizingMaskIntoConstraints = false
        logsTextView.backgroundColor = UIColor(white: 0.05, alpha: 1.0)
        logsTextView.textColor = UIColor(red: 0.0, green: 1.0, blue: 0.0, alpha: 1.0) // Green terminal color
        logsTextView.font = UIFont.monospacedSystemFont(ofSize: 11, weight: .regular)
        logsTextView.isEditable = false
        logsTextView.isScrollEnabled = false
        logsTextView.textContainerInset = UIEdgeInsets(top: 8, left: 12, bottom: 8, right: 12)
        logsTextView.text = section.logs.joined(separator: "\n")
        logsTextView.isHidden = section.isCollapsed
        containerView.addSubview(logsTextView)
        
        let heightConstraint = logsTextView.heightAnchor.constraint(equalToConstant: section.isCollapsed ? 0 : CGFloat(section.logs.count * 16 + 16))
        heightConstraint.priority = .defaultHigh
        
        NSLayoutConstraint.activate([
            headerButton.topAnchor.constraint(equalTo: containerView.topAnchor),
            headerButton.leadingAnchor.constraint(equalTo: containerView.leadingAnchor),
            headerButton.trailingAnchor.constraint(equalTo: containerView.trailingAnchor),
            headerButton.heightAnchor.constraint(equalToConstant: 36),
            
            logsTextView.topAnchor.constraint(equalTo: headerButton.bottomAnchor),
            logsTextView.leadingAnchor.constraint(equalTo: containerView.leadingAnchor),
            logsTextView.trailingAnchor.constraint(equalTo: containerView.trailingAnchor),
            logsTextView.bottomAnchor.constraint(equalTo: containerView.bottomAnchor),
            heightConstraint
        ])
        
        return containerView
    }
    
    @objc private func sectionHeaderTapped(_ sender: UIButton) {
        let index = sender.tag
        guard index < sections.count else { return }
        
        // Toggle section collapse state
        ConsoleManager.shared.toggleSection(at: index)
        
        // Refresh all sections to update the UI
        sections = ConsoleManager.shared.getSections()
        updateSections()
    }
    
    deinit {
        // Cleanup
    }
}

// Helper extension for safe array access
extension Array {
    subscript(safe index: Int) -> Element? {
        return indices.contains(index) ? self[index] : nil
    }
}

