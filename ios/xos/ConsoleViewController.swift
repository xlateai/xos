import UIKit
import Xos

class ConsoleViewController: UIViewController, UIScrollViewDelegate, UITextViewDelegate, ConsoleLogReceiver {
    private var textView: UITextView!
    private var logs: [String] = []
    private var isScrolledToBottom = true
    private var timestampButton: UIButton!
    
    override func viewDidLoad() {
        super.viewDidLoad()
        
        view.backgroundColor = UIColor(white: 0.1, alpha: 1.0)
        
        // Create text view
        textView = UITextView()
        textView.translatesAutoresizingMaskIntoConstraints = false
        textView.backgroundColor = UIColor(white: 0.05, alpha: 1.0)
        textView.textColor = UIColor(red: 0.0, green: 1.0, blue: 0.0, alpha: 1.0) // Green terminal color
        textView.font = UIFont.monospacedSystemFont(ofSize: 12, weight: .regular)
        textView.isEditable = false
        textView.isScrollEnabled = true
        textView.textContainerInset = UIEdgeInsets(top: 16, left: 16, bottom: 16, right: 16)
        textView.delegate = self
        view.addSubview(textView)
        
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
            textView.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 50),
            textView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            textView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            textView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            
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
        logs = ConsoleManager.shared.getLogs()
        updateTextView()
    }
    
    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        ConsoleManager.shared.unregisterConsole()
    }
    
    // MARK: - UIScrollViewDelegate
    
    func scrollViewDidScroll(_ scrollView: UIScrollView) {
        let bottom = textView.contentSize.height - textView.bounds.height
        isScrolledToBottom = textView.contentOffset.y >= bottom - 10
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
    func updateLogs(_ newLogs: [String]) {
        logs = newLogs
        updateTextView()
    }
    
    private func updateTextView() {
        let text = logs.joined(separator: "\n")
        textView.text = text
        
        // Auto-scroll to bottom if user was already at bottom
        if isScrolledToBottom {
            DispatchQueue.main.async { [weak self] in
                guard let self = self else { return }
                let bottom = self.textView.contentSize.height - self.textView.bounds.height
                if bottom > 0 {
                    self.textView.setContentOffset(CGPoint(x: 0, y: bottom), animated: false)
                }
            }
        }
    }
    
    deinit {
        // Cleanup
    }
}

