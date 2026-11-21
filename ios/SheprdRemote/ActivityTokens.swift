import ActivityKit
import Foundation
import SheprdRemoteCore

/// Collects the push tokens ActivityKit hands the app and sends them to the
/// paired computers (`POST /api/apns`): the push-to-start token, which lets
/// a computer start an activity while the app is closed, goes to every
/// computer; each activity's own token goes only to the computer whose CA
/// fingerprint the activity carries as `computerId`, never by name.
///
/// With the push relay on (`PushRelaySetting`), each computer gets push
/// relay handles instead of tokens (`PushRelayAccess`), and with them the
/// notify key it seals Live Activity content to. Tokens go out only while
/// it is off.
///
/// A paired computer can put another computer's id in an activity it
/// starts. That only makes the app hand the other computer a token for an
/// activity the claimant started, which is harmless; the claimant can never
/// obtain a token for an activity another computer started.
@MainActor
@Observable
final class ActivityTokens {
    struct Running: Identifiable, Equatable {
        let id: String
        let attributes: ChatActivityFacts
        var token: String?
    }

    /// How a computer said it delivers pushes when it last took this
    /// phone's registration, for Settings.
    enum Delivery: Equatable {
        case ownKey
        case pushRelay
        /// It answered that push is not set up there.
        case off
        /// An older computer, which does not say.
        case unknown
    }

    /// The setting that keeps this phone's activities generic: every
    /// computer leaves the chat's name and steps out of them.
    static let genericSetting = "liveActivitiesGeneric"

    private(set) var pushToStart: String?
    /// The app's own APNs token, for sealed alerts.
    private(set) var alert: String?
    private(set) var running: [String: Running] = [:]
    private(set) var lastError: String?
    private(set) var delivery: [Fingerprint: Delivery] = [:]

    /// The paired computers, kept current by the app model.
    var computers: [Computer] = []
    let pushRelay = PushRelayAccess()
    private let sessions: Sessions
    private var started = false

    init(sessions: Sessions) {
        self.sessions = sessions
    }

    /// The push relay setting changed. Off, the handles this phone made are
    /// revoked first; either way every computer then gets what it should
    /// use, tokens or handles, which replace what it held.
    func pushRelayChanged(to on: Bool) async {
        do {
            try NotifyKey.mirror()
        } catch {
            lastError = "Live Activity keys: \(error.localizedDescription)"
        }
        if !on { await pushRelay.turnedOff() }
        await sync()
    }

    /// Starts watching ActivityKit; safe to call more than once.
    func start() {
        guard !started else { return }
        started = true
        // The widget reads the keys from the App Group file only while the
        // push relay is on; this writes or removes it to match.
        try? NotifyKey.mirror()
        Task {
            for await data in Activity<ChatActivityAttributes>.pushToStartTokenUpdates {
                pushToStart = data.hexString
                await sync()
            }
        }
        Task {
            for await activity in Activity<ChatActivityAttributes>.activityUpdates {
                watch(activity)
            }
        }
        for activity in Activity<ChatActivityAttributes>.activities {
            watch(activity)
        }
    }

    /// A new APNs token from iOS: every computer gets it, with the key
    /// alerts are sealed to.
    func setAlert(_ token: String) {
        guard token != alert else { return }
        alert = token
        Task { await sync() }
    }

    private func watch(_ activity: Activity<ChatActivityAttributes>) {
        guard running[activity.id] == nil else { return }
        // Sealed attributes open only with the key of the computer they
        // name; one no key opens shows the cover, and its token goes nowhere.
        guard let facts = activity.attributes.facts(keys: NotifyKey.activityKeys()) else { return }
        if facts.namesChat, UserDefaults.standard.bool(forKey: Self.genericSetting) {
            let id = activity.id
            Task { await Self.end(id) }
            return
        }
        running[activity.id] = Running(id: activity.id, attributes: facts, token: activity.pushToken?.hexString)
        Task {
            for await data in activity.pushTokenUpdates {
                running[activity.id]?.token = data.hexString
                if let owner = facts.fingerprint {
                    await sync(only: owner)
                }
            }
        }
        Task {
            for await state in activity.activityStateUpdates where state == .ended || state == .dismissed {
                running[activity.id] = nil
                if let owner = facts.fingerprint {
                    await sync(only: owner)
                }
                return
            }
        }
    }

    /// Tells the computers about Keep Generic. Turned on, it first ends
    /// every activity: a title and workspace are fixed when one starts, so
    /// no later push could take them off the lock screen, and what one last
    /// said may name the chat too. That needs no network, so it does not
    /// wait on a computer that is away; `watch` ends what such a computer
    /// starts before it hears.
    func genericChanged(to generic: Bool) async {
        if generic {
            for id in Activity<ChatActivityAttributes>.activities.map(\.id) {
                await Self.end(id)
            }
        }
        await sync()
    }

    /// Ends the activity with `id` at once. By id and off this actor, since
    /// an activity it holds cannot be sent to ActivityKit.
    private nonisolated static func end(_ id: String) async {
        await Activity<ChatActivityAttributes>.activities.first { $0.id == id }?
            .end(nil, dismissalPolicy: .immediate)
    }

    /// Sends each paired computer (or only the one with CA `only`) its
    /// tokens, or with the push relay on, handles for them.
    func sync(only: Fingerprint? = nil) async {
        let viaPushRelay = PushRelaySetting.isOn
        // Attests first when needed; without a credential no handle can be
        // made, and no token goes out either while the push relay is on.
        let credential = viaPushRelay ? await pushRelay.usableCredential() : nil
        let recipient = { (token: String?, kind: PushRelayKind) -> API.PushRecipient? in
            guard let token else { return nil }
            guard viaPushRelay else { return .token(token) }
            guard let credential, let handle = self.pushRelay.handle(for: token, kind: kind, credential: credential) else {
                return nil
            }
            return .pushRelayHandle(handle, environment: credential.environment)
        }
        for computer in computers where only == nil || computer.fingerprint == only {
            let activities = running.values
                .filter { $0.attributes.fingerprint == computer.fingerprint }
                .compactMap { running in
                    recipient(running.token, .activity).map { API.APNsRegistration.Activity(pane: running.attributes.pane, token: $0) }
                }
            let alert = recipient(alert, .alert)
            // Each computer seals to its own key. Without one, alerts would
            // arrive sealed to nobody: send neither, and the computer keeps
            // using Web Push or nothing. Through the push relay, Live
            // Activity content is sealed to it too.
            let needsKey = alert != nil || viaPushRelay
            let notifyKey = needsKey ? try? NotifyKey.loadOrCreate(for: computer.fingerprint).registered : nil
            let registration = API.APNsRegistration(
                pushToStart: recipient(pushToStart, .pushToStart),
                activities: activities,
                alert: notifyKey == nil ? nil : alert,
                notifyKey: notifyKey,
                genericActivities: UserDefaults.standard.bool(forKey: Self.genericSetting)
            )
            do {
                let session = try await sessions.session(for: computer)
                let answer = try await session.client.registerAPNs(registration, bearer: session.token)
                delivery[computer.fingerprint] = switch answer.delivery {
                case .ownKey: .ownKey
                case .pushRelay: .pushRelay
                case nil: .unknown
                }
                lastError = viaPushRelay && credential == nil ? pushRelay.lastError : nil
            } catch let RemoteError.status(503, _) {
                // A computer with no push set up. Nothing is wrong, and the
                // session is good: it answered.
                delivery[computer.fingerprint] = .off
                lastError = nil
            } catch {
                await sessions.forget(computer)
                lastError = "\(computer.name) (\(computer.fingerprint.hex.prefix(8))): \(error.localizedDescription)"
            }
        }
    }
}
