import ApplicationServices
import CoreGraphics
import Foundation

/// What macOS lets the agent do.
struct Permissions {
    var accessibility: Bool
    var screenRecording: Bool

    /// As this process sees them. A process keeps the answers it read
    /// first, so a permission granted while it runs shows only to a new one.
    static func own() -> Permissions {
        Permissions(accessibility: AXIsProcessTrusted(), screenRecording: CGPreflightScreenCaptureAccess())
    }

    /// As macOS answers now, asked through a fresh copy of the agent
    /// (`--permissions`). When that sees one this process does not, the
    /// agent restarts to take it; launchd starts it again.
    static func current() -> Permissions {
        let own = own()
        guard !(own.accessibility && own.screenRecording), let fresh = fresh() else { return own }
        if fresh.accessibility && !own.accessibility || fresh.screenRecording && !own.screenRecording {
            restartSoon()
        }
        return fresh
    }

    /// `--permissions`'s answer: "1 0" is Accessibility but no Screen Recording.
    var line: String { "\(accessibility ? 1 : 0) \(screenRecording ? 1 : 0)" }

    private static func fresh() -> Permissions? {
        guard let executable = Bundle.main.executableURL else { return nil }
        let child = Process()
        let output = Pipe()
        child.executableURL = executable
        child.arguments = ["--permissions"]
        child.standardOutput = output
        guard (try? child.run()) != nil else { return nil }
        let data = output.fileHandleForReading.readDataToEndOfFile()
        child.waitUntilExit()
        let fields = String(decoding: data, as: UTF8.self).split(separator: " ").map { $0.hasPrefix("1") }
        guard child.terminationStatus == 0, fields.count == 2 else { return nil }
        return Permissions(accessibility: fields[0], screenRecording: fields[1])
    }
}

/// Exits once no request is under way, so launchd starts a process that
/// holds the new permission.
private func restartSoon() {
    Activity.restart()
}

/// Refuses an operation this process may not do yet: macOS's prompt when
/// it was never answered, or a retry once the agent restarts with it.
func require(_ permitted: (Permissions) -> Bool, refusal: Refusal, ask: () -> Void) throws {
    guard !permitted(Permissions.own()) else { return }
    if permitted(Permissions.current()) {
        throw Refusal.failed("Sheprd Agent is restarting to use a permission just allowed; try again in a few seconds")
    }
    ask()
    throw refusal
}
