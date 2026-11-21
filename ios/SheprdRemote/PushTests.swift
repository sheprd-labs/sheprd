import Foundation
import SheprdRemoteCore

/// Settings' "Send test push": asks one paired computer for a sealed test
/// alert (`POST /api/apns/test`), then waits for the Notification Service
/// Extension to record its arrival (`PushTestReceipts`). Says where it
/// stopped: the computer, APNs, or the phone.
@MainActor
@Observable
final class PushTests {
    enum Status: Equatable {
        case sending
        /// APNs took it; the phone has not opened it yet.
        case waiting
        case arrived(seconds: Double)
        case failed(String)
    }

    /// How long an accepted push may take before it counts as lost.
    static let patience: Duration = .seconds(30)

    private(set) var status: [Fingerprint: Status] = [:]
    private let sessions: Sessions
    private let group = UserDefaults(suiteName: "group.so.pen.herdr.remote")

    init(sessions: Sessions) {
        self.sessions = sessions
    }

    func send(to computer: Computer) async {
        let key = computer.fingerprint
        guard status[key] != .sending, status[key] != .waiting else { return }
        status[key] = .sending
        let id = UUID().uuidString
        let started = Date()
        let answer: API.PushTestAnswer
        do {
            let session = try await sessions.session(for: computer)
            answer = try await session.client.sendTestPush(id: id, bearer: session.token)
        } catch let RemoteError.status(503, _) {
            status[key] = .failed(PushRelaySetting.isOn
                ? "\(computer.name) has the push relay off and no APNs key. Turn on the push relay in Sheprd's Remote control settings there."
                : "\(computer.name) has no APNs key, so it cannot send this phone notifications. Turn on the push relay here and on the computer.")
            return
        } catch let RemoteError.status(400, text) where text.contains("not registered") {
            status[key] = .failed(PushRelaySetting.isOn
                ? "\(computer.name) has no push relay handle from this phone that it can use. Allow notifications for Sheprd and reopen the app; a computer with its own APNs key needs the push relay off here."
                : "This phone has not given \(computer.name) its notification token yet. Allow notifications for Sheprd, then reopen the app. A computer using the push relay needs it on here too.")
            return
        } catch {
            status[key] = .failed("Could not ask \(computer.name): \(error.localizedDescription)")
            return
        }
        switch answer.outcome {
        case .delivered:
            status[key] = .waiting
        case .gone:
            status[key] = .failed("APNs no longer knows this phone's token. Reopen the app to send a new one, then try again.")
            return
        case .limited:
            status[key] = .failed("APNs or the push relay is limiting pushes to this phone. Try again in a minute.")
            return
        case .refused where answer.reason == nil && answer.status == 403:
            // The push relay gives no reason; 403 is a handle it no longer
            // takes, which the next registration replaces.
            status[key] = .failed("The push relay no longer takes this phone's handle. Reopen the app to send a new one, then try again.")
            return
        case .refused:
            let reason = answer.reason ?? "HTTP \(answer.status ?? 0)"
            status[key] = .failed("APNs or the push relay refused it: \(reason).")
            return
        case .unreachable:
            status[key] = .failed("\(computer.name) could not reach APNs or the push relay.")
            return
        }
        let deadline = ContinuousClock.now + Self.patience
        while ContinuousClock.now < deadline {
            if let arrival = PushTestReceipts.arrival(of: id, in: group) {
                status[key] = .arrived(seconds: arrival.timeIntervalSince(started))
                return
            }
            try? await Task.sleep(for: .milliseconds(250))
        }
        status[key] = .failed("APNs took it, but it did not arrive within 30 seconds. Check that notifications are on for Sheprd and Focus allows them.")
    }

    /// The line Settings shows under a computer's button.
    static func text(_ status: Status) -> String {
        switch status {
        case .sending: "Sending…"
        case .waiting: "Sent. Waiting for it to arrive…"
        case let .arrived(seconds): "Arrived in \(seconds.formatted(.number.precision(.fractionLength(1)))) s."
        case let .failed(why): why
        }
    }
}
