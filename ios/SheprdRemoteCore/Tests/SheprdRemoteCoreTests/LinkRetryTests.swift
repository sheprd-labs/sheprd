import Foundation
import Testing
@testable import SheprdRemoteCore

/// A computer that stays away keeps the link offline across retries, so
/// the phone's "offline" toast does not come and go with each attempt.
struct LinkRetryTests {
    @Test func aRetryThatFailsStaysOfflineWithoutConnectingInBetween() async {
        let computer = FakeComputer()
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: await computer.io())
        await link.start()
        let first = await collect(link) { $0.contains { if case .status(.offline) = $0 { true } else { false } } }
        #expect(first.first == .status(.connecting))
        await link.networkChanged()
        let retry = await collect(link) { $0.contains { if case .status(.offline) = $0 { true } else { false } } }
        #expect(!retry.contains(.status(.connecting)))
        await computer.setPing("https://lan", mac)
        await link.networkChanged()
        let back = await collect(link) { $0.contains(.status(.online(origin: "https://lan"))) }
        #expect(!back.contains(.status(.connecting)))
        #expect(back.contains(.status(.online(origin: "https://lan"))))
        await link.stop()
    }
}

/// The usual outage: a live socket drops, and then nothing answers.
struct LinkDroppedTests {
    private static func reported(_ updates: [LinkUpdate]) -> Bool {
        updates.contains { if case .unreachable = $0 { true } else { false } }
    }

    @Test func aDroppedSocketReportsTheAddressesOnce() async {
        let computer = FakeComputer()
        await computer.setPing("https://lan", mac)
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: await computer.io())
        await link.start()
        _ = await collect(link) { $0.contains(.status(.online(origin: "https://lan"))) }
        await computer.stopAnswering("https://lan")
        await computer.drop()
        _ = await collect(link) { $0.contains { if case .status(.offline) = $0 { true } else { false } } }
        await link.networkChanged()
        let race = await collect(link) { $0.contains { if case .status(.offline) = $0 { true } else { false } } }
        #expect(Self.reported(race))
        await link.networkChanged()
        let again = await collect(link) { $0.contains { if case .status(.offline) = $0 { true } else { false } } }
        #expect(!Self.reported(again))
        await link.stop()
    }
}

/// A daemon too old for this app answers pings but refuses the socket.
struct LinkOutdatedTests {
    @Test func aDaemonWithoutTheLiveSocketIsOutdatedNeverOnline() async {
        let computer = FakeComputer()
        await computer.setPing("https://lan", mac)
        await computer.failConnects(LinkError.outdated)
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: await computer.io())
        await link.start()
        let first = await collect(link) { $0.contains(.status(.outdated)) }
        #expect(first.contains(.status(.outdated)))
        await link.networkChanged()
        await until { await computer.connects.count == 2 }
        let retry = await collect(link) { $0.contains(.status(.outdated)) }
        #expect(!retry.contains(.status(.connecting)))
        #expect(!retry.contains { if case .status(.online) = $0 { true } else { false } })
        // Once Sheprd there is updated, the next retry gets through.
        await computer.failConnects(nil)
        await link.networkChanged()
        let back = await collect(link) { $0.contains(.status(.online(origin: "https://lan"))) }
        #expect(back.contains(.status(.online(origin: "https://lan"))))
        await link.stop()
    }
}

/// Each address's failure is kept once per outage, for the computer's log.
struct LinkUnreachableTests {
    @Test func goingOfflineSaysWhyEachAddressFailedOnce() async {
        let computer = FakeComputer()
        await computer.setPing("https://other", "AA:BB")
        let origins = ["https://lan", "https://other"]
        let link = ComputerLink(fingerprint: mac, origins: origins, io: await computer.io())
        await link.start()
        let first = await collect(link) { $0.contains { if case .status(.offline) = $0 { true } else { false } } }
        let reports = first.compactMap { if case let .unreachable(failures) = $0 { failures } else { nil } }
        #expect(reports.count == 1)
        #expect(reports.first?.count == 2)
        #expect(reports.first?.contains("https://other: another computer answered") == true)
        #expect(reports.first?.contains { $0.hasPrefix("https://lan: ") } == true)
        await link.networkChanged()
        let retry = await collect(link) { $0.contains { if case .status(.offline) = $0 { true } else { false } } }
        #expect(!retry.contains { if case .unreachable = $0 { true } else { false } })
        await link.stop()
    }
}

/// Relay ban checks run alongside retries and cancel when the network changes.
struct LinkRelayBanTests {
    private actor BanProbe {
        var relay: RelayEndpoint?
        var started = 0
        var cancelled = 0
        private var waitForRelease = false
        private var ignoreCancellation = false
        private var waiters: [CheckedContinuation<Void, Never>] = []

        func configure(relay: RelayEndpoint?, waitForRelease: Bool = false, ignoreCancellation: Bool = false) {
            self.relay = relay
            self.waitForRelease = waitForRelease
            self.ignoreCancellation = ignoreCancellation
        }

        func check() async -> RelayEndpoint? {
            started += 1
            if waitForRelease {
                if ignoreCancellation {
                    await withCheckedContinuation { waiters.append($0) }
                } else {
                    await withTaskCancellationHandler {
                        await withCheckedContinuation { waiters.append($0) }
                    } onCancel: {
                        Task { await self.cancelWaiters() }
                    }
                    if Task.isCancelled {
                        cancelled += 1
                        return nil
                    }
                }
            }
            return relay
        }

        func release() {
            let pending = waiters
            waiters.removeAll()
            pending.forEach { $0.resume() }
        }

        private func cancelWaiters() {
            let pending = waiters
            waiters.removeAll()
            pending.forEach { $0.resume() }
        }
    }

    @Test func aSlowRelayBanCheckDoesNotDelayReconnectOnNetworkChange() async {
        let computer = FakeComputer()
        let probe = BanProbe()
        await probe.configure(relay: nil, waitForRelease: true)
        var io = await computer.io()
        io.bannedRelay = { await probe.check() }
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: io)
        await link.start()

        let offline = await collect(link) { $0.contains { if case .status(.offline) = $0 { true } else { false } } }
        #expect(offline.contains { if case .status(.offline) = $0 { true } else { false } })
        await until { await probe.started == 1 }

        await computer.setPing("https://lan", mac)
        await link.networkChanged()
        let online = await collect(link) { $0.contains(.status(.online(origin: "https://lan"))) }
        #expect(online.contains(.status(.online(origin: "https://lan"))))
        await until { await probe.cancelled == 1 }
        #expect(await probe.cancelled == 1)
        await link.stop()
    }

    @Test func aConfirmedRelayBanUpdatesStatusWhilePausedAndClearsWhenOnline() async throws {
        let relay = try #require(RelayEndpoint("relay.example.org"))
        let computer = FakeComputer()
        let probe = BanProbe()
        await probe.configure(relay: relay, waitForRelease: true)
        var io = await computer.io()
        io.bannedRelay = { await probe.check() }
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: io)
        await link.start()

        _ = await collect(link) { $0.contains { if case .status(.offline) = $0 { true } else { false } } }
        await until { await probe.started == 1 }
        await probe.release()

        let banned = await collect(link) { $0.contains(.status(.relayBanned(relay))) }
        #expect(banned.contains(.status(.relayBanned(relay))))

        await computer.setPing("https://lan", mac)
        await link.networkChanged()
        let online = await collect(link) { $0.contains(.status(.online(origin: "https://lan"))) }
        #expect(online.contains(.status(.online(origin: "https://lan"))))
        await link.stop()
    }

    @Test func aStaleRelayBanCheckCannotOverwriteOnline() async throws {
        let relay = try #require(RelayEndpoint("relay.example.org"))
        let computer = FakeComputer()
        let probe = BanProbe()
        await probe.configure(relay: relay, waitForRelease: true, ignoreCancellation: true)
        var io = await computer.io()
        io.bannedRelay = { await probe.check() }
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: io)
        await link.start()

        _ = await collect(link) { $0.contains { if case .status(.offline) = $0 { true } else { false } } }
        await until { await probe.started == 1 }

        await computer.setPing("https://lan", mac)
        await link.networkChanged()
        _ = await collect(link) { $0.contains(.status(.online(origin: "https://lan"))) }

        await probe.release()
        await computer.emit(.error("marker"))
        let after = await collect(link) { $0.contains(.event(.error("marker"))) }
        #expect(!after.contains(.status(.relayBanned(relay))))
        await link.stop()
    }

    @Test func aLiftedRelayBanReturnsToOffline() async throws {
        let relay = try #require(RelayEndpoint("relay.example.org"))
        let computer = FakeComputer()
        let probe = BanProbe()
        await probe.configure(relay: relay)
        var io = await computer.io()
        io.bannedRelay = { await probe.check() }
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: io)
        await link.start()

        let banned = await collect(link) { $0.contains(.status(.relayBanned(relay))) }
        #expect(banned.contains(.status(.relayBanned(relay))))

        await probe.configure(relay: nil)
        await link.networkChanged()
        let offline = await collect(link) { $0.contains { if case .status(.offline) = $0 { true } else { false } } }
        #expect(offline.contains { if case .status(.offline) = $0 { true } else { false } })
        await link.stop()
    }
}
