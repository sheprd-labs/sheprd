import SheprdRemoteCore
import XCTest
@testable import SheprdRemote

@MainActor
final class PairingAndVoiceTests: XCTestCase {
    func testCancelledAttemptCannotSaveOrReplaceANewerPairing() async {
        for replace in [false, true] {
            let pairing = PairingModel()
            // This exchange completes even after cancellation, like an in-flight response.
            let (started, signal) = AsyncStream<CheckedContinuation<Void, Never>>.makeStream()
            let old = pairing.start({ _ in XCTFail("Saved a cancelled pairing") }) {
                await withCheckedContinuation { signal.yield($0) }
                return DemoData.studio
            }
            var iterator = started.makeAsyncIterator()
            guard let finish = await iterator.next() else { return XCTFail("Exchange did not start") }
            pairing.cancel()
            if replace {
                await pairing.start({ _ in }) { DemoData.laptop }.value
            }
            finish.resume()
            await old.value
            XCTAssertEqual(pairing.step, replace ? .paired(DemoData.laptop.name) : .idle)
            XCTAssertEqual(pairing.paired, replace ? DemoData.laptop : nil)
        }
    }

    func testReadBackFailureLeavesPlanAvailableForManualConfirmation() async throws {
        let defaults = UserDefaults.standard
        let previous = defaults.object(forKey: VoiceSettings.confirmsKey)
        defaults.set(true, forKey: VoiceSettings.confirmsKey)
        defer { defaults.set(previous, forKey: VoiceSettings.confirmsKey) }
        let model = AppModel()
        model.startDemo()
        let options = Data(#"{"agents":[{"id":"claude","name":"Claude"}],"recent":["/demo/site"],"roots":[]}"#.utf8)
        model.voiceOptions[DemoData.studio.device] = try JSONDecoder().decode(NewOptions.self, from: options)
        let denied = SpeechFailure.response(401)
        let session = VoiceSession(model: model, loadSpeechAccount: { throw denied })
        await session.begin(saying: "New worktree on studio in site about caching with Claude")
        XCTAssertTrue(session.plan.confirmableByVoice)
        XCTAssertEqual(session.phase, .ready)
        XCTAssertEqual(session.speechError, denied.localizedDescription)
        XCTAssertFalse(session.awaitingYes)
        session.cancel()
    }
}
