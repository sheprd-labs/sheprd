import Foundation

/// The largest request the agent reads, and the largest reply it sends.
let maxRequest = 1 << 20
let maxReply = 64 << 20

/// Accepts the host's connections on `port` forever, one thread each.
func listen(port: UInt32) -> Never {
    let server = socket(AF_VSOCK, SOCK_STREAM, 0)
    guard server >= 0 else { fatal("socket") }
    var address = sockaddr_vm()
    address.svm_len = UInt8(MemoryLayout<sockaddr_vm>.size)
    address.svm_family = sa_family_t(AF_VSOCK)
    address.svm_port = port
    address.svm_cid = UInt32(VMADDR_CID_ANY)
    let bound = withUnsafePointer(to: &address) {
        $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
            bind(server, $0, socklen_t(MemoryLayout<sockaddr_vm>.size))
        }
    }
    guard bound == 0, Darwin.listen(server, 8) == 0 else { fatal("bind") }
    print("listening on vsock port \(port)")
    while true {
        var peer = sockaddr_vm()
        var length = socklen_t(MemoryLayout<sockaddr_vm>.size)
        let connection = withUnsafeMutablePointer(to: &peer) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { accept(server, $0, &length) }
        }
        guard connection >= 0 else { continue }
        // Only the host: a process in this guest that reached the agent
        // would borrow its Accessibility and Screen Recording permissions.
        guard peer.svm_cid == UInt32(VMADDR_CID_HOST) else {
            close(connection)
            continue
        }
        Thread.detachNewThread { serve(connection) }
    }
}

func fatal(_ what: String) -> Never {
    print("\(what) failed: \(String(cString: strerror(errno)))")
    exit(1)
}

/// Answers the one request on `connection`, then closes it.
func serve(_ connection: Int32) {
    defer { close(connection) }
    guard let length = readLength(connection), length <= maxRequest,
          let body = readExactly(connection, length) else { return }
    var reply: [String: Any]
    do {
        guard let request = try JSONSerialization.jsonObject(with: body) as? [String: Any] else {
            throw Refusal.invalid("a request is a JSON object")
        }
        reply = try handle(request)
        reply["ok"] = true
    } catch let refusal as Refusal {
        reply = refusal.reply
    } catch {
        reply = Refusal.invalid("\(error)").reply
    }
    guard var data = try? JSONSerialization.data(withJSONObject: reply) else { return }
    if data.count > maxReply {
        data = (try? JSONSerialization.data(withJSONObject: Refusal.failed("reply too large").reply)) ?? Data()
    }
    var prefix = UInt32(data.count).bigEndian
    _ = writeAll(connection, Data(bytes: &prefix, count: 4) + data)
}

func handle(_ request: [String: Any]) throws -> [String: Any] {
    switch request["op"] as? String {
    case "hello": return hello()
    case "exec": return try run(request)
    case "screenshot": return try screenshot()
    case "click": return try click(request)
    case "type": return try type(request)
    case "key": return try key(request)
    default: throw Refusal.invalid("unknown op")
    }
}

/// Why a request was not done.
enum Refusal: Error {
    case accessibility, screenRecording
    case invalid(String), failed(String)

    var reply: [String: Any] {
        switch self {
        case .accessibility:
            return ["ok": false, "error": "accessibility",
                    "message": "allow Sheprd Agent in System Settings > Privacy & Security > Accessibility"]
        case .screenRecording:
            return ["ok": false, "error": "screen_recording",
                    "message": "allow Sheprd Agent in System Settings > Privacy & Security > Screen & System Audio Recording"]
        case .invalid(let message): return ["ok": false, "error": "invalid", "message": message]
        case .failed(let message): return ["ok": false, "error": "failed", "message": message]
        }
    }
}

func readLength(_ fd: Int32) -> Int? {
    guard let bytes = readExactly(fd, 4) else { return nil }
    return bytes.reduce(0) { $0 << 8 | Int($1) }
}

func readExactly(_ fd: Int32, _ count: Int) -> Data? {
    var data = Data(count: count)
    var done = 0
    while done < count {
        let got = data.withUnsafeMutableBytes { read(fd, $0.baseAddress! + done, count - done) }
        guard got > 0 else { return nil }
        done += got
    }
    return data
}

func writeAll(_ fd: Int32, _ data: Data) -> Bool {
    data.withUnsafeBytes { buffer in
        var done = 0
        while done < buffer.count {
            let wrote = write(fd, buffer.baseAddress! + done, buffer.count - done)
            guard wrote > 0 else { return false }
            done += wrote
        }
        return true
    }
}
