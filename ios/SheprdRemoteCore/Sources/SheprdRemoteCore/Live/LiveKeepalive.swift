/// Whether a live socket's path still carries traffic.
public struct LiveKeepalive {
    /// Allows the phone's maximum 16 MiB message at the server's minimum
    /// 16 KiB/s, plus its 10-second write allowance and scheduling margin.
    /// HTTP probes never renew this budget; only a socket pong does.
    public static let unansweredLimit: Duration = .seconds(18 * 60)

    private var unansweredSince: ContinuousClock.Instant?

    public init() {}

    /// What a keepalive round found.
    public enum Finding: Equatable, Sendable {
        /// The pong came in time.
        case answered
        /// The address answers independently, and the socket still has
        /// time to finish a large message that its pong cannot overtake.
        case busy
        /// The address failed or the socket exhausted its pong deadline.
        case dead
    }

    /// Waits for `pong`, and only when it does not come, asks `probe`,
    /// which reaches the same address over a connection of its own.
    public mutating func check(
        now: () -> ContinuousClock.Instant = { .now },
        pong: () async -> Bool,
        probe: () async -> Bool
    ) async -> Finding {
        let started = now()
        if let unansweredSince, started - unansweredSince >= Self.unansweredLimit {
            return .dead
        }
        if await pong() {
            unansweredSince = nil
            return .answered
        }
        let since = unansweredSince ?? started
        unansweredSince = since
        guard now() - since < Self.unansweredLimit else { return .dead }
        guard await probe() else { return .dead }
        return now() - since < Self.unansweredLimit ? .busy : .dead
    }
}
