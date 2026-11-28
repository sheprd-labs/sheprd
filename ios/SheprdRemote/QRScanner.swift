@preconcurrency import AVFoundation
import SwiftUI
import UIKit

/// The camera, reading the first QR code it sees. Shows why when there is
/// no camera (the simulator) or the user declined access.
struct QRScanner: UIViewControllerRepresentable {
    let onCode: (String) -> Void

    func makeUIViewController(context: Context) -> ScannerController {
        ScannerController(onCode: onCode)
    }

    func updateUIViewController(_ controller: ScannerController, context: Context) {}
}

// The metadata delegate is called on the main queue (`queue: .main`); the
// preconcurrency conformance checks that at run time.
final class ScannerController: UIViewController, @preconcurrency AVCaptureMetadataOutputObjectsDelegate {
    private let onCode: (String) -> Void
    private let session = AVCaptureSession()
    /// Serial queue ordering `startRunning()` and `stopRunning()` so dismissal
    /// always stops an already-queued start.
    private let sessionQueue = DispatchQueue(label: "so.pen.herdr.remote.qr.session", qos: .userInitiated)
    private var preview: AVCaptureVideoPreviewLayer?
    private var reported = false
    private var stopped = false

    init(onCode: @escaping (String) -> Void) {
        self.onCode = onCode
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used from a storyboard")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .black
        Task { await setUp() }
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        preview?.frame = view.bounds
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        stopped = true
        let session = session
        sessionQueue.async { session.stopRunning() }
    }

    private func setUp() async {
        guard await AVCaptureDevice.requestAccess(for: .video) else {
            return show("Camera access is off. Allow it in Settings, or paste the link below.")
        }
        guard !stopped else { return }
        guard let camera = AVCaptureDevice.default(for: .video),
              let input = try? AVCaptureDeviceInput(device: camera),
              session.canAddInput(input)
        else { return show("No camera here. Paste the pairing link below.") }
        session.addInput(input)
        let output = AVCaptureMetadataOutput()
        guard session.canAddOutput(output) else { return show("The camera cannot read codes.") }
        session.addOutput(output)
        guard output.availableMetadataObjectTypes.contains(.qr) else {
            session.removeOutput(output)
            return show("This camera cannot read QR codes. Pick a computer under Nearby above.")
        }
        output.setMetadataObjectsDelegate(self, queue: .main)
        output.metadataObjectTypes = [.qr]

        let preview = AVCaptureVideoPreviewLayer(session: session)
        preview.videoGravity = .resizeAspectFill
        preview.frame = view.bounds
        view.layer.addSublayer(preview)
        self.preview = preview
        // startRunning blocks until the camera is up.
        guard !stopped else { return }
        let session = session
        sessionQueue.async { session.startRunning() }
    }

    private func show(_ text: String) {
        let label = UILabel()
        label.text = text
        label.textColor = .white
        label.numberOfLines = 0
        label.textAlignment = .center
        label.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(label)
        NSLayoutConstraint.activate([
            label.centerYAnchor.constraint(equalTo: view.centerYAnchor),
            label.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 24),
            label.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -24),
        ])
    }

    func metadataOutput(
        _ output: AVCaptureMetadataOutput,
        didOutput objects: [AVMetadataObject],
        from connection: AVCaptureConnection
    ) {
        guard !reported,
              let code = objects.compactMap({ $0 as? AVMetadataMachineReadableCodeObject }).first?.stringValue
        else { return }
        reported = true
        let session = session
        sessionQueue.async { session.stopRunning() }
        onCode(code)
    }
}
