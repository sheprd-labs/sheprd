import Foundation

/// What `exec` keeps of the end of each stream; the rest is read and dropped.
let maxOutput = 1 << 20
let defaultTimeout = 120_000

/// Runs a command as the logged-in user, with the PATH a Terminal gets.
func run(_ request: [String: Any]) throws -> [String: Any] {
    guard let argv = request["argv"] as? [String], let program = argv.first, !program.isEmpty else {
        throw Refusal.invalid("exec needs argv")
    }
    let timeout = (request["timeout_ms"] as? Int) ?? defaultTimeout
    let process = Process()
    if program.contains("/") {
        process.executableURL = URL(fileURLWithPath: program)
        process.arguments = Array(argv.dropFirst())
    } else {
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = argv
    }
    var environment = ProcessInfo.processInfo.environment
    environment["PATH"] = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
    process.environment = environment
    let home = FileManager.default.homeDirectoryForCurrentUser
    if let cwd = request["cwd"] as? String {
        let path = cwd == "~" ? home.path : cwd.hasPrefix("~/") ? home.path + cwd.dropFirst(1) : cwd
        var directory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: path, isDirectory: &directory), directory.boolValue else {
            throw Refusal.invalid("no directory \(path)")
        }
        process.currentDirectoryURL = URL(fileURLWithPath: path)
    } else {
        process.currentDirectoryURL = home
    }
    process.standardInput = FileHandle.nullDevice
    let stdout = Capture(), stderr = Capture()
    process.standardOutput = stdout.pipe
    process.standardError = stderr.pipe
    do {
        try process.run()
    } catch {
        throw Refusal.failed("cannot run \(program): \(error.localizedDescription)")
    }
    let finished = DispatchSemaphore(value: 0)
    let waiter = Thread {
        process.waitUntilExit()
        finished.signal()
    }
    waiter.start()
    var timedOut = false
    if finished.wait(timeout: .now() + .milliseconds(max(1, timeout))) == .timedOut {
        timedOut = true
        process.terminate()
        if finished.wait(timeout: .now() + .seconds(2)) == .timedOut {
            kill(process.processIdentifier, SIGKILL)
            finished.wait()
        }
    }
    let (out, outCut) = stdout.finish()
    let (err, errCut) = stderr.finish()
    var reply: [String: Any] = [
        "stdout": String(decoding: out, as: UTF8.self),
        "stderr": String(decoding: err, as: UTF8.self),
        "truncated": outCut || errCut,
        "timed_out": timedOut,
    ]
    // A command a signal ended has no exit code.
    if process.terminationReason == .exit {
        reply["status"] = Int(process.terminationStatus)
    } else {
        reply["signal"] = Int(process.terminationStatus)
    }
    return reply
}

/// Drains one of a process's streams on its own thread, keeping the last
/// `maxOutput` bytes.
final class Capture: @unchecked Sendable {
    let pipe = Pipe()
    private var data = Data()
    private var cut = false
    private let done = DispatchSemaphore(value: 0)

    init() {
        let handle = pipe.fileHandleForReading
        Thread.detachNewThread { [self] in
            while true {
                let chunk = handle.availableData
                if chunk.isEmpty { break }
                data.append(chunk)
                if data.count > maxOutput {
                    data.removeFirst(data.count - maxOutput)
                    cut = true
                }
            }
            done.signal()
        }
    }

    func finish() -> (Data, Bool) {
        done.wait()
        return (data, cut)
    }
}
