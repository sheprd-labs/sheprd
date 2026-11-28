import SheprdRemoteCore
import XCTest

final class AgentInteractionTests: XCTestCase {
    func testParsesUrgentYubiKeyAndCue() {
        let text = """
        Rebased cleanly.
        :::urgent Tap your YubiKey
        preset: yubikey
        Waiting on git push origin main
        :::
        ::cue yubikey
        """
        let msg = AgentMessage.parse(text)
        XCTAssertEqual(msg.interactions.count, 1)
        let first = msg.interactions[0]
        XCTAssertEqual(first.directive, .urgent)
        XCTAssertEqual(first.scope, .all)
        XCTAssertEqual(first.title, "Tap your YubiKey")
        XCTAssertEqual(first.body, "Waiting on git push origin main")
        XCTAssertEqual(first.cue, .yubikey)
        XCTAssertEqual(msg.cues, [.yubikey])
    }

    func testParsesSecretVaultAndRunbookAndProgress() {
        let text = """
        :::secret Provide STRIPE_TEST_KEY
        key: STRIPE_TEST_KEY
        dest: env-file:.env.test.local
        Needed to run webhook tests.
        :::

        :::runbook Ship v0.18.0
        - [x] agent: Run just ci
        - [ ] you: Tap YubiKey
        :::

        :::progress Running 412 tests
        percent: 68%
        step: chat::effects
        :::
        """
        let msg = AgentMessage.parse(text)
        XCTAssertEqual(msg.interactions.count, 3)

        if case let .secret(key, dest, saveKeychain) = msg.interactions[0].payload {
            XCTAssertEqual(key, "STRIPE_TEST_KEY")
            XCTAssertEqual(dest, "env-file:.env.test.local")
            XCTAssertTrue(saveKeychain)
        } else {
            XCTFail("expected secret payload")
        }

        if case let .runbook(steps) = msg.interactions[1].payload {
            XCTAssertFalse(msg.interactions[1].autoClear)
            XCTAssertEqual(steps.count, 2)
            XCTAssertEqual(steps[0], RunbookStep(label: "Run just ci", human: false, done: true))
            XCTAssertEqual(steps[1], RunbookStep(label: "Tap YubiKey", human: true, done: false))
        } else {
            XCTFail("expected runbook payload")
        }

        if case let .progress(percent, step, done) = msg.interactions[2].payload {
            XCTAssertFalse(msg.interactions[2].autoClear)
            XCTAssertEqual(percent, 68)
            XCTAssertEqual(step, "chat::effects")
            XCTAssertFalse(done)
        } else {
            XCTFail("expected progress payload")
        }
    }

    func testChatEffectsPlaysInteractionCuesOnceAndSkipsHistory() {
        let now = Date(timeIntervalSince1970: 1_800_000_000)
        let entry = Entry(id: EntryId(1), kind: .agent(text: ":::urgent Tap YubiKey\nWaiting\n:::\n::cue done"))
        var transcript = Transcript(agent: "Claude")
        transcript.entries = [entry]

        var historyEffects = ChatEffects()
        historyEffects.history(transcript)
        XCTAssertTrue(historyEffects.live(transcript, changed: [entry.id], now: now).isEmpty)

        var liveEffects = ChatEffects()
        let first = liveEffects.live(transcript, changed: [entry.id], now: now)
        XCTAssertEqual(first, [.interactionCue(.yubikey), .interactionCue(.done)])

        // Streaming update of the same entry does not replay already-played cues.
        let second = liveEffects.live(transcript, changed: [entry.id], now: now.addingTimeInterval(1))
        XCTAssertTrue(second.isEmpty)
    }

    func testRestoringSavedMarkPlaysNewInteractionCuesStreamedWhileAway() throws {
        let now = Date(timeIntervalSince1970: 1_800_000_000)
        let epoch = UUID()

        // User was viewing the entry while it only contained plain prose (0 cues).
        var before = Transcript(agent: "Claude")
        before.entries = [Entry(id: EntryId(1), kind: .agent(text: "Starting push…"))]
        var initial = ChatEffects()
        initial.history(before, epoch: epoch, mark: nil)
        _ = initial.catchUp(before, now: now)
        let savedMark = try XCTUnwrap(initial.mark)
        XCTAssertEqual(savedMark.interactionCues, 0)

        // While disconnected, an urgent block streamed into that same entry.
        var after = Transcript(agent: "Claude")
        after.entries = [Entry(id: EntryId(1), kind: .agent(text: "Starting push…\n:::urgent Tap YubiKey\nWaiting\n:::"))]

        var reopened = ChatEffects()
        reopened.history(after, epoch: epoch, mark: savedMark)
        let caughtUp = reopened.catchUp(after, now: now.addingTimeInterval(5))
        XCTAssertEqual(caughtUp, [.interactionCue(.yubikey)])
        XCTAssertEqual(reopened.mark?.interactionCues, 1)

        // Legacy cached Mark JSON without `interactionCues` still decodes cleanly.
        let legacyJSON = #"{"epoch":null,"entry":1,"bubbles":0,"screen":false}"#
        let decoded = try JSONDecoder().decode(ChatEffects.Mark.self, from: Data(legacyJSON.utf8))
        XCTAssertEqual(decoded.interactionCues, 0)
    }

    func testIgnoresDirectivesInsideCodeFencesAndStripsRowLine() {
        let fenced = "```md\n:::urgent Not real\n:::\n::cue yubikey\n```"
        let msg = AgentMessage.parse(fenced)
        XCTAssertTrue(msg.interactions.isEmpty)
        XCTAssertTrue(msg.cues.isEmpty)

        XCTAssertEqual(
            AgentMessage.rowLine(":::urgent Tap your YubiKey :::"),
            "Tap your YubiKey"
        )
    }

    func testEveryInteractionCueHasAudioAndHaptics() {
        for cue in InteractionCue.allCases {
            XCTAssertFalse(cue.cue.sound.voices.isEmpty, "\(cue)")
            XCTAssertFalse(cue.cue.taps.isEmpty, "\(cue)")
        }
    }
}
