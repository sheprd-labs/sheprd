/// The relay's answer to a dial or a listener's message: snake_case text
/// in WebSocket JSON, one byte over QUIC.
public enum RelayStatus: String, Codable, CaseIterable, Sendable {
    // Wire values: never renumber. In byte order.
    case ok
    /// No one listens on the route, or not for this dial key.
    case offline
    /// The route or the relay holds as many sessions as it may.
    case busy
    /// The listener did not accept in time.
    case timeout
    /// Too many requests from this address.
    case limited
    /// A proof did not hold.
    case refused
    /// The listen token was missing or wrong.
    case forbidden
    /// The relay, or this user, holds as many listeners as it may.
    case full
    /// The request could not be read.
    case malformed
    /// More pairing keys at once than allowed.
    case tooMany = "too_many"

    /// The status as the QUIC protocol's byte.
    public var byte: UInt8 {
        switch self {
        case .ok: 0
        case .offline: 1
        case .busy: 2
        case .timeout: 3
        case .limited: 4
        case .refused: 5
        case .forbidden: 6
        case .full: 7
        case .malformed: 8
        case .tooMany: 9
        }
    }
}

/// WebSocket close codes a peer may see from the relay, in the private range.
public enum RelayCloseCode: Int, CaseIterable, Sendable {
    // keep-sorted start
    /// One side's address was banned for misbehaving.
    case banned = 4429
    /// The session reached its longest allowed length.
    case expired = 4410
    /// The relay holds as many listeners as it may.
    case full = 4503
    /// Nothing crossed the session for the idle timeout.
    case idle = 4408
    /// No route by that name, or no listener on it.
    case offline = 4404
    /// The other side closed or broke.
    case peerGone = 4000
    /// A text frame, or a frame larger than allowed.
    case protocolViolation = 4400
    /// The listener's proof or token was refused.
    case refused = 4401
    /// Another socket proved the same key and took over the route.
    case replaced = 4409
    /// The listener stopped allowing the dial key this session used.
    case revoked = 4403
    // keep-sorted end

    /// The snake_case name the relay's documentation and vectors use.
    public var name: String {
        switch self {
        case .banned: "banned"
        case .expired: "expired"
        case .full: "full"
        case .idle: "idle"
        case .offline: "offline"
        case .peerGone: "peer_gone"
        case .protocolViolation: "protocol"
        case .refused: "refused"
        case .replaced: "replaced"
        case .revoked: "revoked"
        }
    }
}
