import Foundation
import Testing
@testable import SheprdRemoteCore

/// A chat followed while no screen shows it says so to the computer, on
/// every subscribe, until a screen opens it.
struct LinkBackgroundTests {
    @Test func aBackgroundChatStaysBackgroundAcrossAReconnectUntilShown() async {
        let computer = FakeComputer()
        await computer.setPing("https://lan", mac)
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: await computer.io())
        await link.watch(.chat(pane: "p1"), background: true)
        await link.start()
        await until { await computer.sent.count == 1 }
        #expect(await computer.sent.last == .subscribe(.chat(pane: "p1"), background: true))
        await computer.drop()
        await link.networkChanged()
        await until { await computer.sent.count == 2 }
        #expect(await computer.sent.last == .subscribe(.chat(pane: "p1"), background: true))
        await link.watch(.chat(pane: "p1"))
        await until { await computer.sent.count == 3 }
        #expect(await computer.sent.last == .subscribe(.chat(pane: "p1")))
        await link.stop()
    }

    @Test func onlyABackgroundSubscribeCarriesTheFlag() throws {
        let shown = try JSONSerialization.jsonObject(with: JSONEncoder().encode(LiveRequest.subscribe(.chat(pane: "p1")))) as? [String: Any]
        #expect(shown?["background"] == nil)
        let warm = try JSONSerialization.jsonObject(with: JSONEncoder().encode(LiveRequest.subscribe(.chat(pane: "p1"), background: true))) as? [String: Any]
        #expect(warm?["background"] as? Bool == true)
    }
}
