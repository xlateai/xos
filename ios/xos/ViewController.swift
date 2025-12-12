import UIKit

class ViewController: UIViewController {
    private var viewportView: XosViewportView!
    private var appName: String = "blank"
    
    override func viewDidLoad() {
        super.viewDidLoad()
        
        view.backgroundColor = .black
        
        // Create viewport view
        viewportView = XosViewportView(frame: view.bounds)
        viewportView.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        viewportView.setAppName(appName)
        view.addSubview(viewportView)
        
        // Add app selector button (optional, for testing different apps)
        let button = UIButton(type: .system)
        button.setTitle("Change App", for: .normal)
        button.addTarget(self, action: #selector(changeApp), for: .touchUpInside)
        button.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(button)
        
        NSLayoutConstraint.activate([
            button.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 20),
            button.trailingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.trailingAnchor, constant: -20)
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
}

