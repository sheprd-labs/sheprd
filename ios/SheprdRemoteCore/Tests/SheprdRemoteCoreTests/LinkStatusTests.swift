import Foundation
import Testing
@testable import SheprdRemoteCore

struct LinkStatusTests {
    @Test func reconnectKeepsTheLastConfirmedStatusUntilItChanges() {
        let online = LinkStatus.online(origin: "https://computer")
        let offline = LinkStatus.offline(reason: "No address answered")

        #expect(LinkStatus.connecting.presented(lastKnown: online) == online)
        #expect(LinkStatus.connecting.presented(lastKnown: offline) == offline)
        #expect(LinkStatus.connecting.presented(lastKnown: nil) == .connecting)
        #expect(offline.presented(lastKnown: online) == offline)
        #expect(online.presented(lastKnown: offline) == online)
        #expect(LinkStatus.revoked.presented(lastKnown: online) == .revoked)
    }

    @Test func lastStatusCanBeRestoredAfterAppLaunch() throws {
        let statuses: [LinkStatus] = [
            .online(origin: "https://computer"),
            .offline(reason: "No address answered"),
            .revoked,
            .outdated,
        ]
        for status in statuses {
            let data = try JSONEncoder().encode(status)
            #expect(try JSONDecoder().decode(LinkStatus.self, from: data) == status)
        }
    }
}
