import Testing
@testable import SheprdGitHub

/// Reads of the pull request lists: joined while they ask the same, and
/// replaced once what they ask, or the account, changes. Each step waits
/// for the one before it, so no test depends on which task starts first.
@MainActor
struct LatestReadTests {
    /// What the reads did, and whether they may finish.
    @MainActor
    final class Log {
        var started: [String] = []
        var kept: [String] = []
        var released = false
    }

    /// A read that waits to be released, and keeps its answer only if it
    /// was not replaced meanwhile.
    private static func read(_ name: String, _ log: Log) -> @MainActor @Sendable () async -> Void {
        {
            log.started.append(name)
            while !log.released, !Task.isCancelled { try? await Task.sleep(for: .milliseconds(2)) }
            if !Task.isCancelled { log.kept.append(name) }
        }
    }

    private static func until(_ condition: @MainActor () -> Bool) async {
        while !condition() { try? await Task.sleep(for: .milliseconds(2)) }
    }

    @Test func theSameKeyJoinsTheReadUnderWay() async {
        let reads = LatestRead<String>()
        let log = Log()
        let first = Task { await reads.run("open", Self.read("a", log)) }
        await Self.until { log.started == ["a"] }
        let second = Task { await reads.run("open", Self.read("b", log)) }
        await Self.until { reads.callers == 2 }
        log.released = true
        await first.value
        await second.value
        #expect(log.started == ["a"])
        #expect(log.kept == ["a"])
        #expect(!reads.isRunning)
    }

    @Test func anotherKeyReplacesTheReadUnderWay() async {
        let reads = LatestRead<String>()
        let log = Log()
        let first = Task {
            await reads.run("lent account", Self.read("a", log))
            return reads.isRunning
        }
        await Self.until { log.started == ["a"] }
        let second = Task { await reads.run("own account", Self.read("b", log)) }
        await Self.until { log.started == ["a", "b"] }
        // The replaced read's caller returns while the new read runs.
        #expect(await first.value)
        log.released = true
        await second.value
        // The replaced read's answer is never kept.
        #expect(log.kept == ["b"])
        #expect(!reads.isRunning)
    }
}
