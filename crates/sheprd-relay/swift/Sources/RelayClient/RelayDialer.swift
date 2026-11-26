import CryptoKit
import Foundation

/// Why a dial did not reach its listener.
public enum RelayDialError: Error, Equatable, Sendable {
    // keep-sorted start
    /// The relay refused the WebSocket upgrade because this address was
    /// banned for misbehaving.
    case banned
    /// The relay closed the WebSocket with this code (see ``RelayCloseCode``).
    case closed(code: Int)
    /// The relay said something that is not the protocol.
    case protocolViolation
    /// The relay answered the dial with this status.
    case refused(RelayStatus)
    /// The relay answered with a status this client does not know.
    case unknownStatus(String)
    // keep-sorted end
}

/// Dials a listener's route through a relay's WebSocket protocol:
///
/// 1. Relay: `{"t":"challenge","nonce","version":1}`.
/// 2. Dialer: `{"key","signature"}` over ``RelayProof/dialSigned(name:nonce:route:)``.
/// 3. Relay: `{"t":"status","status"}`; after `ok`, binary frames are the
///    session.
public struct RelayDialer: Sendable {
    /// The protocol version this client speaks.
    public static let protocolVersion = 1

    public let address: RelayAddress
    public let session: URLSession
    /// The largest WebSocket message the connection accepts.
    public var maximumMessageSize: Int
    /// The largest binary frame ``RelayConnection/sendMessage(_:)`` writes.
    /// A relay refuses frames over its `max-frame`, 64 KiB by default and
    /// never under 1 KiB; lower it for a relay set below the default.
    public var maximumFrameSize: Int

    public init(
        address: RelayAddress,
        session: URLSession = .shared,
        maximumMessageSize: Int = 1 << 20,
        maximumFrameSize: Int = RelayConnection.defaultFrameSize
    ) {
        self.address = address
        self.session = session
        self.maximumMessageSize = maximumMessageSize
        self.maximumFrameSize = maximumFrameSize
    }

    /// Dials `route` (32 bytes) as the holder of `key`. Cancelling the task
    /// cancels the socket, so a dial waiting on the relay ends at once.
    public func dial(route: Data, key: P256.Signing.PrivateKey) async throws -> RelayConnection {
        // No Origin header: the relay is not a web page's to reach.
        let socket = session.webSocketTask(with: URLRequest(url: address.dial(route: route)))
        socket.maximumMessageSize = maximumMessageSize
        socket.resume()
        let proofName = address.proofName
        let maximumFrameSize = maximumFrameSize
        return try await withTaskCancellationHandler {
            do {
                let challenge = try await Self.receiveText(socket)
                let hello = try Self.hello(challenge: challenge, proofName: proofName, route: route, key: key)
                try await socket.send(.string(hello))
                try Self.check(status: try await Self.receiveText(socket))
                return RelayConnection(socket: socket, maximumFrameSize: maximumFrameSize)
            } catch {
                socket.cancel(with: .goingAway, reason: nil)
                throw RelayConnection.mapped(error, socket: socket)
            }
        } onCancel: {
            socket.cancel(with: .goingAway, reason: nil)
        }
    }

    private static func receiveText(_ socket: URLSessionWebSocketTask) async throws -> String {
        guard case let .string(text) = try await socket.receive() else { throw RelayDialError.protocolViolation }
        return text
    }

    // MARK: The handshake's messages, without a network.

    /// The nonce in the relay's challenge text.
    public static func nonce(challenge text: String) throws -> Data {
        struct Challenge: Decodable {
            let t: String
            let nonce: String
            let version: Int
        }
        guard let challenge = try? JSONDecoder().decode(Challenge.self, from: Data(text.utf8)),
              challenge.t == "challenge", challenge.version == protocolVersion,
              let nonce = Base64URL.decode(challenge.nonce), nonce.count == RelayProof.nonceLength
        else { throw RelayDialError.protocolViolation }
        return nonce
    }

    /// The hello text answering the relay's `challenge` text, for a dial of
    /// `route` on the relay named `proofName`.
    public static func hello(
        challenge: String, proofName: String, route: Data, key: P256.Signing.PrivateKey
    ) throws -> String {
        let signed = RelayProof.dialSigned(name: proofName, nonce: try nonce(challenge: challenge), route: route)
        return try RelayProof.Hello(key: key, signed: signed).text()
    }

    /// The status the relay's status text carries, as it spelled it.
    public static func statusText(_ text: String) throws -> String {
        struct Status: Decodable {
            let t: String
            let status: String
        }
        guard let status = try? JSONDecoder().decode(Status.self, from: Data(text.utf8)), status.t == "status"
        else { throw RelayDialError.protocolViolation }
        return status.status
    }

    /// The status in the relay's status text.
    public static func status(_ text: String) throws -> RelayStatus {
        let spelled = try statusText(text)
        guard let status = RelayStatus(rawValue: spelled) else { throw RelayDialError.unknownStatus(spelled) }
        return status
    }

    /// Whether the relay's HTTP answer to the upgrade marks an address ban:
    /// a 403 with `relay-banned: 1`, or the older `sheprd-relay-banned: 1`.
    public static func isBan(_ response: HTTPURLResponse) -> Bool {
        response.statusCode == 403 && ["relay-banned", "sheprd-relay-banned"].contains { name in
            response.value(forHTTPHeaderField: name)?.trimmingCharacters(in: .whitespaces) == "1"
        }
    }

    /// Throws unless the relay's status text says `ok`.
    public static func check(status text: String) throws {
        let status = try status(text)
        guard status == .ok else { throw RelayDialError.refused(status) }
    }
}
