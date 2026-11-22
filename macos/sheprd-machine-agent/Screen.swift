import ApplicationServices
import CoreGraphics
import Foundation

/// The main display: its size in pixels, and pixels per point.
func display() -> (width: Int, height: Int, scale: Double) {
    let id = CGMainDisplayID()
    let points = CGDisplayPixelsWide(id)
    guard let mode = CGDisplayCopyDisplayMode(id), points > 0 else {
        return (CGDisplayPixelsWide(id), CGDisplayPixelsHigh(id), 1)
    }
    return (mode.pixelWidth, mode.pixelHeight, Double(mode.pixelWidth) / Double(points))
}

func hello() -> [String: Any] {
    let (width, height, scale) = display()
    let permissions = Permissions.current()
    return [
        "version": agentVersion,
        "width": width,
        "height": height,
        "scale": scale,
        "accessibility": permissions.accessibility,
        "screen_recording": permissions.screenRecording,
    ]
}

/// The whole main display as a PNG, through macOS's own `screencapture`.
func screenshot() throws -> [String: Any] {
    try require(\.screenRecording, refusal: .screenRecording) { _ = CGRequestScreenCaptureAccess() }
    let path = FileManager.default.temporaryDirectory
        .appendingPathComponent("sheprd-\(UUID().uuidString).png")
    defer { try? FileManager.default.removeItem(at: path) }
    let capture = Process()
    capture.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
    capture.arguments = ["-x", "-m", "-t", "png", path.path]
    do {
        try capture.run()
    } catch {
        throw Refusal.failed("cannot run screencapture: \(error.localizedDescription)")
    }
    capture.waitUntilExit()
    guard capture.terminationStatus == 0, let png = try? Data(contentsOf: path), png.count > 24 else {
        throw Refusal.failed("screencapture made no image")
    }
    // A PNG's IHDR holds its width and height, big-endian, at bytes 16 and 20.
    let width = png[16..<20].reduce(0) { $0 << 8 | Int($1) }
    let height = png[20..<24].reduce(0) { $0 << 8 | Int($1) }
    return ["png": png.base64EncodedString(), "width": width, "height": height]
}
