import Testing
@testable import SheprdRemoteCore

/// A pong cannot overtake a large message on its way to the phone, so a
/// late one allows a bounded grace period when the address still answers.
struct LiveKeepaliveTests {
    @Test func aPongInTimeAsksNothingElse() async {
        var health = LiveKeepalive()
        var probed = false
        let finding = await health.check(pong: { true }, probe: { probed = true; return true })
        #expect(finding == .answered)
        #expect(!probed)
    }

    @Test func aLatePongOnABusySocketKeepsIt() async {
        var health = LiveKeepalive()
        #expect(await health.check(pong: { false }, probe: { true }) == .busy)
    }

    @Test func noPongAndNoAnswerIsADeadPath() async {
        var health = LiveKeepalive()
        #expect(await health.check(pong: { false }, probe: { false }) == .dead)
    }

    @Test func successfulProbesCannotRenewTheSocketDeadline() async {
        var health = LiveKeepalive()
        let start = ContinuousClock.now
        var elapsed = Duration.zero
        while elapsed < LiveKeepalive.unansweredLimit {
            #expect(await health.check(
                now: { start + elapsed }, pong: { false }, probe: { true }
            ) == .busy)
            elapsed += .seconds(15)
        }
        var called = false
        #expect(await health.check(
            now: { start + LiveKeepalive.unansweredLimit },
            pong: { called = true; return false }, probe: { called = true; return true }
        ) == .dead)
        #expect(!called)
    }

    @Test func aPongRenewsTheBudgetForTheNextLargeMessage() async {
        var health = LiveKeepalive()
        let start = ContinuousClock.now
        #expect(await health.check(now: { start }, pong: { false }, probe: { true }) == .busy)
        #expect(await health.check(
            now: { start + LiveKeepalive.unansweredLimit - .seconds(1) }, pong: { true }, probe: { false }
        ) == .answered)
        #expect(await health.check(
            now: { start + LiveKeepalive.unansweredLimit }, pong: { false }, probe: { true }
        ) == .busy)
    }

    @Test func probeFinishingAfterTheDeadlineCannotKeepTheSocket() async {
        var health = LiveKeepalive()
        let start = ContinuousClock.now
        var now = start
        #expect(await health.check(
            now: { now }, pong: { false },
            probe: { now = start + LiveKeepalive.unansweredLimit; return true }
        ) == .dead)
    }
}
