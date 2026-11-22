import Foundation

/// What `exec` keeps of the end of each stream; the rest is read and dropped.
let maxOutput = 1 << 20
let defaultTimeout = 120_000
/// How long a command has to go once asked to, and how long its output may
/// stay open after it ended or was stopped.
private let grace: DispatchTimeInterval = .seconds(2)

/// Runs a command as the logged-in user, with the PATH a Terminal gets, in a
/// process group of its own: a timeout stops everything it started.
func run(_ request: [String: Any]) throws -> [String: Any] {
    guard let argv = request["argv"] as? [String], let program = argv.first, !program.isEmpty else {
        throw Refusal.invalid("exec needs argv")
    }
    let timeout = (request["timeout_ms"] as? Int) ?? defaultTimeout
    let directory = try folder(request["cwd"] as? String)
    // A path is the program itself, relative to the command's folder; a
    // bare name is looked up on PATH.
    let (path, arguments): (String, [String]) = program.contains("/")
        ? (program.hasPrefix("/") ? program : directory + "/" + program, Array(argv.dropFirst()))
        : ("/usr/bin/env", argv)
    var environment = ProcessInfo.processInfo.environment
    environment["PATH"] = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
    let stdout = Capture(), stderr = Capture()
    let pid = try spawn(
        path, [path] + arguments,
        environment: environment.map { "\($0.key)=\($0.value)" },
        directory: directory, stdout: stdout, stderr: stderr)
    let deadline = DispatchTime.now() + .milliseconds(max(1, timeout))
    let exited = DispatchSemaphore(value: 0)
    let end = End()
    Thread.detachNewThread {
        while waitpid(pid, &end.status, 0) < 0 && errno == EINTR {}
        exited.signal()
    }
    var timedOut = false
    if exited.wait(timeout: deadline) == .timedOut {
        timedOut = true
        kill(-pid, SIGTERM)
        let ended = exited.wait(timeout: .now() + grace) == .success
        // What ignored that goes now, whether or not the command itself
        // ended: a group outlives its leader while any member runs, and its
        // ID is not reused meanwhile.
        kill(-pid, SIGKILL)
        // The waiting thread signals once, so wait again only if that wait
        // did not take its signal.
        if !ended {
            exited.wait()
        }
    }
    // What the command left running may hold its output open: it goes
    // once the time is up.
    let drained = DispatchTime.now() > deadline ? DispatchTime.now() + grace : deadline
    if !(stdout.wait(until: drained) && stderr.wait(until: drained)) {
        kill(-pid, SIGKILL)
        _ = stdout.wait(until: .now() + grace)
        _ = stderr.wait(until: .now() + grace)
    }
    let (out, outCut) = stdout.snapshot()
    let (err, errCut) = stderr.snapshot()
    var reply: [String: Any] = [
        "stdout": String(decoding: out, as: UTF8.self),
        "stderr": String(decoding: err, as: UTF8.self),
        "truncated": outCut || errCut,
        "timed_out": timedOut,
    ]
    // A command a signal ended has no exit code.
    let signal = end.status & 0x7f
    if signal == 0 {
        reply["status"] = Int((end.status >> 8) & 0xff)
    } else {
        reply["signal"] = Int(signal)
    }
    return reply
}

/// How a command ended, as `waitpid` says. Written by the thread that
/// waits, and read only after it signalled.
private final class End: @unchecked Sendable {
    var status: Int32 = 0
}

/// The folder a command runs in: `cwd`, with `~` for home, or home.
private func folder(_ cwd: String?) throws -> String {
    let home = FileManager.default.homeDirectoryForCurrentUser.path
    guard let cwd else { return home }
    let path = cwd == "~" ? home : cwd.hasPrefix("~/") ? home + cwd.dropFirst(1) : cwd
    var directory: ObjCBool = false
    guard FileManager.default.fileExists(atPath: path, isDirectory: &directory), directory.boolValue else {
        throw Refusal.invalid("no directory \(path)")
    }
    return path
}

/// Starts `path` as the leader of a new process group, with no input, its
/// output in the captures, and none of the agent's other descriptors (the
/// host's connections among them).
private func spawn(
    _ path: String, _ argv: [String], environment: [String], directory: String,
    stdout: Capture, stderr: Capture
) throws -> pid_t {
    var actions: posix_spawn_file_actions_t?
    var attributes: posix_spawnattr_t?
    posix_spawn_file_actions_init(&actions)
    posix_spawnattr_init(&attributes)
    defer {
        posix_spawn_file_actions_destroy(&actions)
        posix_spawnattr_destroy(&attributes)
    }
    posix_spawn_file_actions_addopen(&actions, 0, "/dev/null", O_RDONLY, 0)
    posix_spawn_file_actions_adddup2(&actions, stdout.pipe.fileHandleForWriting.fileDescriptor, 1)
    posix_spawn_file_actions_adddup2(&actions, stderr.pipe.fileHandleForWriting.fileDescriptor, 2)
    posix_spawn_file_actions_addchdir_np(&actions, directory)
    posix_spawnattr_setflags(&attributes, Int16(POSIX_SPAWN_SETPGROUP | POSIX_SPAWN_CLOEXEC_DEFAULT))
    posix_spawnattr_setpgroup(&attributes, 0)
    let cArgv = argv.map { strdup($0) } + [nil]
    let cEnvironment = environment.map { strdup($0) } + [nil]
    defer {
        cArgv.forEach { free($0) }
        cEnvironment.forEach { free($0) }
    }
    var pid: pid_t = 0
    let spawned = posix_spawn(&pid, path, &actions, &attributes, cArgv, cEnvironment)
    // The command holds the writing ends now; the agent's copies would keep
    // the output open after it ends.
    stdout.pipe.fileHandleForWriting.closeFile()
    stderr.pipe.fileHandleForWriting.closeFile()
    guard spawned == 0 else {
        throw Refusal.failed("cannot run \(argv[0]): \(String(cString: strerror(spawned)))")
    }
    return pid
}

/// Drains one of a process's streams on its own thread, keeping the last
/// `maxOutput` bytes.
final class Capture: @unchecked Sendable {
    let pipe = Pipe()
    private let lock = NSLock()
    private var data = Data()
    private var cut = false
    private let done = DispatchSemaphore(value: 0)

    init() {
        let handle = pipe.fileHandleForReading
        Thread.detachNewThread { [self] in
            while true {
                let chunk = handle.availableData
                if chunk.isEmpty { break }
                lock.lock()
                data.append(chunk)
                if data.count > maxOutput {
                    data.removeFirst(data.count - maxOutput)
                    cut = true
                }
                lock.unlock()
            }
            done.signal()
        }
    }

    /// Whether the stream ended by `deadline`. Once it has, it stays so.
    func wait(until deadline: DispatchTime) -> Bool {
        guard done.wait(timeout: deadline) == .success else { return false }
        done.signal()
        return true
    }

    /// What was read so far, and whether its start was dropped.
    func snapshot() -> (Data, Bool) {
        lock.lock()
        defer { lock.unlock() }
        return (data, cut)
    }
}
