import UIKit
import Xos

class ViewController: UIViewController {
    private var viewportView: XosViewportView!
    private var appName: String = {
        // Try to get app name from Info.plist (set during build)
        if let defaultApp = Bundle.main.infoDictionary?["XOSDefaultApp"] as? String,
           !defaultApp.isEmpty && defaultApp != "$(XOS_DEFAULT_APP)" {
            return defaultApp
        }
        // Fallback to blank
        return "blank"
    }()
    
    override func viewDidLoad() {
        super.viewDidLoad()
        
        view.backgroundColor = .black
        
        // Create viewport view
        viewportView = XosViewportView(frame: view.bounds)
        viewportView.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        viewportView.setAppName(appName)
        view.addSubview(viewportView)
        
        // Add app selector button (optional, for testing different apps)
        let changeAppButton = UIButton(type: .system)
        changeAppButton.setTitle("Change App", for: .normal)
        changeAppButton.addTarget(self, action: #selector(changeApp), for: .touchUpInside)
        changeAppButton.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(changeAppButton)
        
        // Add console button
        let consoleButton = UIButton(type: .system)
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
    }
    
    @objc private func changeApp() {
        let alert = UIAlertController(title: "Select App", message: nil, preferredStyle: .actionSheet)
        
        let apps = ["blank", "ball", "tracers", "camera", "whiteboard", "waveform", "scroll", "text", "wireframe", "triangles", "cursor", "audiovis", "partitions"]
        
        for app in apps {
            alert.addAction(UIAlertAction(title: app, style: .default) { [weak self] _ in
                self?.appName = app
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
}

