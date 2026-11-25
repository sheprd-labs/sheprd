import Foundation

/// A session through the relay once it let the dial through: bytes, which
/// the relay passes along untouched, in binary WebSocket frames. Peers that
/// want messages can use ``sendMessage(_:)`` and ``receiveMessage()``,
/// which frame them with ``RelayFraming``; do not mix the two on one
/// connection's reading side, and read from one task at a time.
///
/// The relay counts WebSocket pings as session activity, so ``ping()``
/// keeps an idle session alive without any in-band message.
public actor RelayConnection {
    private let socket: URLSessionWebSocketTask
    private var deframer: RelayFraming.Deframer
    private var closed = false

    init(socket: URLSessionWebSocketTask, messageCap: Int = RelayFraming.maxMessage) {
        self.socket = socket
        deframer = RelayFraming.Deframer(cap: messageCap)
    }

    /// Sends `bytes` as one binary frame.
    public func send(_ bytes: Data) async throws {
        guard !closed else { throw RelayDialError.closed(code: URLSessionWebSocketTask.CloseCode.normalClosure.rawValue) }
        do {
            try await socket.send(.data(bytes))
        } catch {
            throw Self.mapped(error, socket: socket)
        }
    }

    /// The next binary frame, or `nil` once the session closed normally.
    /// Cancelling the task cancels the socket: a WebSocket read cannot be
    /// abandoned and resumed without losing its place.
    public func receive() async throws -> Data? {
        guard !closed else { return nil }
        let socket = socket
        do {
            let message = try await withTaskCancellationHandler {
                try await socket.receive()
            } onCancel: {
                socket.cancel(with: .goingAway, reason: nil)
            }
            switch message {
            case let .data(bytes): return bytes
            case .string: throw RelayDialError.protocolViolation
            @unknown default: throw RelayDialError.protocolViolation
            }
        } catch {
            if closed || socket.closeCode == .normalClosure { return nil }
            throw Self.mapped(error, socket: socket)
        }
    }

    /// Sends `message` framed: its length, then the message.
    public func sendMessage(_ message: Data) async throws {
        try await send(RelayFraming.encode(message))
    }

    /// The next framed message, or `nil` once the session closed normally
    /// between messages.
    public func receiveMessage() async throws -> Data? {
        while true {
            if let message = try deframer.next() { return message }
            guard let bytes = try await receive() else {
                guard deframer.buffered == 0 else { throw RelayDialError.protocolViolation }
                return nil
            }
            if let message = try deframer.push(bytes) { return message }
        }
    }

    /// Pings the relay and waits for its pong.
    public func ping() async throws {
        let socket = socket
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            socket.sendPing { error in
                if let error { continuation.resume(throwing: error) } else { continuation.resume() }
            }
        }
    }

    /// Closes the WebSocket.
    public func close() {
        guard !closed else { return }
        closed = true
        socket.cancel(with: .normalClosure, reason: nil)
    }

    /// A socket error as this client reports it: cancellation, a ban, the
    /// relay's close code, or the error itself.
    static func mapped(_ error: Error, socket: URLSessionWebSocketTask) -> Error {
        if error is RelayDialError || error is RelayFraming.Error { return error }
        if Task.isCancelled { return CancellationError() }
        if let response = socket.response as? HTTPURLResponse, RelayDialer.isBan(response) {
            return RelayDialError.banned
        }
        let code = socket.closeCode
        if code != .invalid { return RelayDialError.closed(code: code.rawValue) }
        return error
    }
}
