import Testing
@testable import SheprdRemoteCore

/// The daemon refuses an agent switch while a turn is open, so the phone
/// locks the agent tiles whenever either the status or the turn says so.
struct TranscriptAnsweringTests {
    @Test func idleWithoutATurnIsNotAnswering() {
        var transcript = Transcript(agent: "Claude")
        transcript.status = .idle
        #expect(!transcript.answering)
    }

    @Test func workingStatusIsAnswering() {
        var transcript = Transcript(agent: "Claude")
        transcript.status = .working
        #expect(transcript.answering)
    }

    @Test func anOpenTurnIsAnsweringBeforeTheStatusCatchesUp() {
        var transcript = Transcript(agent: "Claude")
        transcript.status = .idle
        transcript.turn = Turn(startedMs: 1)
        #expect(transcript.answering)
    }
}
