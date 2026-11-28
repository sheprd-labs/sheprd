import Foundation

/// One read at a time, for the latest thing asked: a call with the key of
/// the read under way joins it, and one with another key cancels it and
/// starts its own. The key holds everything an answer depends on (what is
/// asked, and with which account), so an answer for something no longer
/// wanted is never kept by a caller that joined it.
@MainActor
public final class LatestRead<Key: Equatable> {
    private var current: (key: Key, id: UInt64, task: Task<Void, Never>)?
    private var started: UInt64 = 0
    /// Calls waiting on a read, so tests can tell a caller has joined.
    private(set) var callers = 0

    public init() {}

    /// Whether a read is under way.
    public var isRunning: Bool { current != nil }

    /// Runs `work` for `key`, or joins the read of the same key under way,
    /// and returns once that read ends. A replaced read is cancelled: its
    /// `work` must check `Task.isCancelled` before keeping its answer.
    public func run(_ key: Key, _ work: @escaping @MainActor @Sendable () async -> Void) async {
        callers += 1
        defer { callers -= 1 }
        let id: UInt64
        let task: Task<Void, Never>
        if let current, current.key == key {
            (id, task) = (current.id, current.task)
        } else {
            current?.task.cancel()
            started += 1
            id = started
            task = Task { await work() }
            current = (key, id, task)
        }
        await task.value
        if current?.id == id { current = nil }
    }
}
