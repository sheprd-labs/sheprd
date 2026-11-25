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
/// notify key it seals Live Activity content to. Computers push only
/// through the push relay, so with it off no token goes out at all: a
/// computer would keep it and never send to it.
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
        case pushRelay
        /// It answered that push is not set up there.
        case off
        /// An older computer, which does not say or names a mode this
        /// phone does not know.
        case unknown
    }

    private(set) var pushToStart: String?
    /// The app's own APNs token, for sealed alerts.
    private(set) var alert: String?
    private(set) var running: [String: Running] = [:]
    private(set) var lastError: String?
    private(set) var delivery: [Fingerprint: Delivery] = [:]

    /// The paired computers, kept current by the app model.
    var computers: [Computer] = []
    /// The relay proxy address for a computer's device id, while one
    /// listens: a fleet member or linked host may answer only through it.
    @ObservationIgnored var relayOrigin: (String) -> String? = { _ in nil }
    /// Computers the last registration did not reach. Each is registered
    /// again once it comes online (`online`), so a computer away when a
    /// token or the push relay setting changed still gets them.
    private(set) var unreached: Set<Fingerprint> = []
    let pushRelay = PushRelayAccess()
    private static let epochKey = "liveActivityResetEpoch"
    private var activityEpoch: UUID? {
        UserDefaults.standard.string(forKey: Self.epochKey).flatMap(UUID.init(uuidString:))
    }

    private let sessions: Sessions
    private var started = false
    /// Each computer's sync running or last queued; the next for that
    /// computer waits for it.
    private var syncing: [Fingerprint: Task<String?, Never>] = [:]

    init(sessions: Sessions) {
        self.sessions = sessions
    }

    /// The push relay setting changed. Off, the handles this phone made are
    /// revoked first; either way every computer then gets what it should
    /// use, handles or nothing, which replace what it held.
    func pushRelayChanged(to on: Bool) async {
        do {
            try NotifyKey.mirror()
        } catch {
            lastError = "Live Activity keys: \(error.localizedDescription)"
        }
        if !on {
            await resetSharedGroup()
            await pushRelay.turnedOff()
        }
        await sync()
    }

    /// The push relay refused a handle `computer` held: attests again and
    /// sends it new handles. Answers why the computer did not get them, or
    /// `nil` when it did.
    func pushRelayRejected(by computer: Computer) async -> String? {
        pushRelay.rejected()
        return await sync(only: computer.fingerprint)[computer.fingerprint]
    }

    /// `computer` is online again: registers with it when the last try
    /// did not reach it. A try still running may have started before the
    /// link found its relay proxy, so it is waited for, and registered
    /// again when it too misses: the link sends no second online event.
    func online(_ computer: Computer) {
        let fingerprint = computer.fingerprint
        if unreached.contains(fingerprint) {
            Task { await sync(only: fingerprint) }
            return
        }
        guard let running = syncing[fingerprint] else { return }
        Task {
            _ = await running.value
            if unreached.contains(fingerprint) { await sync(only: fingerprint) }
        }
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
        if case let .sharedCard(card) = activity.attributes.content {
            watchShared(activity, card: card)
            return
        }
        guard running[activity.id] == nil else { return }
        // Sealed attributes open only with the key of the computer they
        // name; one no key opens shows the cover, and its token goes nowhere.
        guard let facts = activity.attributes.facts(keys: NotifyKey.activityKeys()) else { return }
        if ActivityGrouping.isRetiredHostCard(facts.pane, since: epoch(for: facts.fingerprint))
            || (facts.namesChat && LiveActivityStyleSetting.current.asksGeneric) {
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

    private var sharedWatches: [String: String] = [:]
    private var bindings: [String: (token: String, task: Task<Void, Never>)] = [:]

    private func watchShared(_ activity: Activity<ChatActivityAttributes>, card: SharedActivityCard) {
        guard PushRelaySetting.isOn, card.belongs(to: ChatActivityAttributes.sharedGroup) else {
            let id = activity.id
            Task { await Self.end(id) }
            return
        }
        if let watched = sharedWatches[card.card] {
            if watched != activity.id {
                let duplicate = activity.id
                Task { await Self.end(duplicate) }
            }
            return
        }
        sharedWatches[card.card] = activity.id
        refreshSharedTokens()
        let tokens = Task {
            for await token in activity.pushTokenUpdates {
                queueBinding(token.hexString, activity: activity.id, card: card)
            }
        }
        Task {
            for await state in activity.activityStateUpdates where state == .ended || state == .dismissed {
                tokens.cancel()
                bindings.removeValue(forKey: activity.id)?.task.cancel()
                sharedWatches[card.card] = nil
                return
            }
        }
    }

    /// Reconcile existing tokens on foreground and after a new card arrives.
    /// This also retires an orphan when a relay restart lost its coordinator.
    func refreshSharedTokens() {
        for activity in Activity<ChatActivityAttributes>.activities {
            guard case let .sharedCard(card) = activity.attributes.content,
                  sharedWatches[card.card] == activity.id, let token = activity.pushToken,
                  activity.activityState == .active || activity.activityState == .stale else { continue }
            queueBinding(token.hexString, activity: activity.id, card: card)
        }
    }

    private func queueBinding(_ token: String, activity id: String, card: SharedActivityCard) {
        let previous = bindings[id]
        guard previous?.token != token else { return }
        previous?.task.cancel()
        let task = Task {
            // A token can rotate while the previous POST is in flight. Keep
            // the posts ordered and let a rotation interrupt a retry delay.
            await previous?.task.value
            guard !Task.isCancelled else { return }
            await bindShared(token, activity: id, card: card)
            if !Task.isCancelled { bindings[id] = nil }
        }
        bindings[id] = (token, task)
    }

    private func bindShared(_ token: String, activity id: String, card: SharedActivityCard) async {
        var delay = 0
        while !Task.isCancelled {
            if delay > 0 { try? await Task.sleep(for: .seconds(delay)) }
            delay = min(60, max(1, delay * 2))
            guard !Task.isCancelled, PushRelaySetting.isOn,
                  let group = ChatActivityAttributes.sharedGroup, card.belongs(to: group) else { return }
            guard let credential = await pushRelay.usableCredential() else { continue }
            guard !Task.isCancelled, group == ChatActivityAttributes.sharedGroup,
                  let handle = pushRelay.handle(for: token, kind: .activity, credential: credential, group: group, slot: card.card)
            else { return }
            let result = try? await pushRelay.push(.bindSharedActivity(handle))
            if result == .delivered { return }
            if result == .gone, !Task.isCancelled {
                await Self.end(id)
                return
            }
        }
    }

    private func resetSharedGroup() async {
        let retired = ChatActivityAttributes.sharedGroup
        LiveActivityStyleSetting.store?.removeObject(forKey: ChatActivityAttributes.sharedGroupKey)
        for activity in Activity<ChatActivityAttributes>.activities {
            if case .sharedCard = activity.attributes.content { await Self.end(activity.id) }
        }
        if let retired { Task { await pushRelay.retire(group: retired) } }
    }

    func groupingChanged(sharedChanged: Bool, affected: Fingerprint? = nil) async {
        if sharedChanged {
            await resetSharedGroup()
            // Only participating computers need replacement host snapshots.
            let epoch = UUID().uuidString
            for computer in computers {
                let fingerprint = computer.fingerprint
                let key = "sharedActivityEpoch." + fingerprint.hex
                if affected == fingerprint || UserDefaults.standard.object(forKey: ActivityGrouping.key(for: fingerprint)) == nil {
                    UserDefaults.standard.set(epoch, forKey: key)
                    for item in running.values where item.attributes.fingerprint == fingerprint && item.attributes.pane.hasPrefix("host-activity/") {
                        await Self.end(item.id)
                    }
                }
            }
        }
        await sync()
    }

    private func epoch(for owner: Fingerprint?) -> UUID? {
        guard let owner else { return activityEpoch }
        return UserDefaults.standard.string(forKey: "sharedActivityEpoch." + owner.hex)
            .flatMap(UUID.init(uuidString:)) ?? activityEpoch
    }

    /// The Live Activity style changed from `old`: running activities are
    /// drawn again in the new one, and the computers hear when Private, the
    /// generic style, was turned on or off.
    func styleChanged(from old: LiveActivityStyle, to new: LiveActivityStyle) async {
        await Self.redraw()
        guard old.asksGeneric != new.asksGeneric else { return }
        await genericChanged(to: new.asksGeneric)
    }

    /// Draws every running activity again: ActivityKit redraws one only
    /// when it is updated, so each is updated with what it already shows.
    private nonisolated static func redraw() async {
        for activity in Activity<ChatActivityAttributes>.activities {
            await activity.update(activity.content)
        }
    }

    /// Tells the computers about the Private style. Turned on, it first ends
    /// every activity: a title and workspace are fixed when one starts, so
    /// no later push could take them off the lock screen, and what one last
    /// said may name the chat too. That needs no network, so it does not
    /// wait on a computer that is away; `watch` ends what such a computer
    /// starts before it hears.
    private func genericChanged(to generic: Bool) async {
        if generic {
            await resetSharedGroup()
            for computer in computers {
                UserDefaults.standard.removeObject(forKey: "sharedActivityEpoch." + computer.fingerprint.hex)
            }
            for id in Activity<ChatActivityAttributes>.activities.map(\.id) {
                await Self.end(id)
                running[id] = nil
            }
            // A missing token also occurs during a normal push-to-start.
            // Explicitly invalidate old host cards, including on offline hosts.
            UserDefaults.standard.set(UUID().uuidString, forKey: Self.epochKey)
        }
        await sync()
    }

    /// Ends the activity with `id` at once. By id and off this actor, since
    /// an activity it holds cannot be sent to ActivityKit.
    private nonisolated static func end(_ id: String) async {
        await Activity<ChatActivityAttributes>.activities.first { $0.id == id }?
            .end(nil, dismissalPolicy: .immediate)
    }

    /// Sends each paired computer (or only the one with CA `only`) push
    /// relay handles for its tokens while the push relay is on, and answers
    /// why each computer that did not take them did not. Syncs for one
    /// computer run one at a time, in the order asked, so one started under
    /// the other setting cannot finish after it; different computers do
    /// not wait on each other, so one that is away delays no other.
    @discardableResult
    func sync(only: Fingerprint? = nil) async -> [Fingerprint: String] {
        var queued: [(Fingerprint, Task<String?, Never>)] = []
        for computer in computers where only == nil || computer.fingerprint == only {
            let previous = syncing[computer.fingerprint]
            let fingerprint = computer.fingerprint
            let task = Task {
                _ = await previous?.value
                return await self.register(fingerprint)
            }
            syncing[computer.fingerprint] = task
            queued.append((computer.fingerprint, task))
        }
        var failures: [Fingerprint: String] = [:]
        for (fingerprint, task) in queued {
            failures[fingerprint] = await task.value
        }
        return failures
    }

    /// Registers with the paired computer with CA `fingerprint`, answering
    /// why it failed, or `nil`. It looks the computer up again after each
    /// wait, so one forgotten meanwhile gets nothing and no notify key is
    /// made again for it. It stops when the push relay setting changes
    /// under it, so no token goes out after it was turned on and no handle
    /// after it was turned off; `pushRelayChanged` queues the sync for the
    /// new setting.
    private func register(_ fingerprint: Fingerprint) async -> String? {
        let forgotten = "This computer is no longer paired."
        let changed = "The push relay setting changed while registering."
        guard computers.contains(where: { $0.fingerprint == fingerprint }) else { return forgotten }
        let viaPushRelay = PushRelaySetting.isOn
        // Attests first when needed; without a credential no handle can be
        // made, and no token goes out either while the push relay is on.
        let credential = viaPushRelay ? await pushRelay.usableCredential() : nil
        guard PushRelaySetting.isOn == viaPushRelay else { return changed }
        guard let computer = computers.first(where: { $0.fingerprint == fingerprint }) else { return forgotten }
        do {
            let relay = relayOrigin(computer.device).map { [$0] } ?? []
            let session = try await sessions.session(for: computer, fallback: relay)
            guard PushRelaySetting.isOn == viaPushRelay else { return changed }
            guard computers.contains(where: { $0.fingerprint == fingerprint }) else { return forgotten }
            // Session setup can suspend while Private ends cards and changes
            // the epoch. Snapshot the entire request after the last setup wait.
            let request = registration(for: fingerprint, viaPushRelay: viaPushRelay, credential: credential)
            let answer = try await session.client.registerAPNs(
                request.body, singleActivity: request.single, epoch: request.epoch, bearer: session.token
            )
            unreached.remove(computer.fingerprint)
            delivery[computer.fingerprint] = switch answer.delivery {
            case .pushRelay: .pushRelay
            case nil: .unknown
            }
            // Through the push relay, no credential means the computer got
            // no handles at all.
            let failure = viaPushRelay && credential == nil
                ? pushRelay.lastError ?? "The push relay gave this phone no credential."
                : nil
            lastError = failure
            return failure
        } catch RemoteError.status(503, _) {
            // A computer with no push set up. Nothing is wrong, and the
            // session is good: it answered.
            delivery[computer.fingerprint] = .off
            unreached.remove(computer.fingerprint)
            lastError = nil
            return nil
        } catch {
            await sessions.forget(computer)
            unreached.insert(computer.fingerprint)
            let failure = "\(computer.name) (\(computer.fingerprint.hex.prefix(8))): \(error.localizedDescription)"
            lastError = failure
            return failure
        }
    }

    /// All fields on one registration describe the same main-actor instant.
    private func registration(
        for fingerprint: Fingerprint, viaPushRelay: Bool, credential: PushRelayCredential?
    ) -> (body: API.APNsRegistration, single: Bool, epoch: UUID?) {
        let recipient = { (token: String?, kind: PushRelayKind) -> API.PushRecipient? in
            // Without the push relay nothing reaches a token, so none goes out.
            guard let token, viaPushRelay else { return nil }
            guard let credential, let handle = self.pushRelay.handle(for: token, kind: kind, credential: credential) else {
                return nil
            }
            return .pushRelayHandle(handle, environment: credential.environment)
        }
        var activities = running.values
            .filter { $0.attributes.fingerprint == fingerprint }
            .compactMap { running in
                recipient(running.token, .activity).map { API.APNsRegistration.Activity(pane: running.attributes.pane, token: $0) }
            }
        if viaPushRelay, ActivityGrouping.shared(for: fingerprint), let pushToStart, let credential,
           let store = LiveActivityStyleSetting.store {
            let group = ChatActivityAttributes.sharedGroup ?? PushRelayCredential.newHandleId()
            store.set(group, forKey: ChatActivityAttributes.sharedGroupKey)
            let slot = SharedActivityCard.slot(group: group, computer: fingerprint)
            if let handle = pushRelay.handle(for: pushToStart, kind: .pushToStart, credential: credential, group: group, slot: slot) {
                activities.append(.init(pane: "relay/all-hosts", token: .pushRelayHandle(handle, environment: credential.environment)))
            }
        }
        let alert = recipient(alert, .alert)
        // Each computer seals to its own notify key. Without one, alerts
        // would arrive sealed to nobody: send neither, and the computer
        // sends this phone nothing through APNs. Through the push relay,
        // Live Activity content is sealed to it too.
        let needsKey = alert != nil || viaPushRelay
        let notifyKey = needsKey ? try? NotifyKey.loadOrCreate(for: fingerprint).registered : nil
        let generic = LiveActivityStyleSetting.current.asksGeneric
        // With the push relay off, the computer drops every handle it holds,
        // so it stops pushing even if revoking them at the push relay failed.
        let registration = viaPushRelay
            ? API.APNsRegistration(
                pushToStart: recipient(pushToStart, .pushToStart),
                activities: activities,
                alert: notifyKey == nil ? nil : alert,
                notifyKey: notifyKey,
                genericActivities: generic
            )
            : .pushRelayOff(genericActivities: generic)
        return (registration, ActivityGrouping.single(for: fingerprint), epoch(for: fingerprint))
    }
}
