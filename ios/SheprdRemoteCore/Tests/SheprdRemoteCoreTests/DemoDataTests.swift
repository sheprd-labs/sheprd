import Foundation
import Testing
@testable import SheprdRemoteCore

/// The demo App Review sees without a computer: every part must decode, or
/// the app shows an empty demo.
struct DemoDataTests {
    @Test func everyComputerHasAHome() throws {
        for computer in DemoData.computers {
            let home = try #require(DemoData.home(of: computer.device))
            #expect(home.computer == computer.name)
            #expect(home.fingerprint == computer.fingerprint.hex)
            #expect(!home.needsUpdate)
            #expect(!home.workspaces.flatMap(\.chats).isEmpty)
        }
        #expect(DemoData.home(of: "someone-else") == nil)
    }

    @Test func worktreeAndAgentLabelsAppearTogether() throws {
        let home = try #require(DemoData.home(of: DemoData.studio.device))
        let workspace = try #require(home.workspaces.first)
        let agent = try #require(workspace.chats.first)
        #expect(workspace.rowLabels(agent: agent.labels) == ["Release", "Widgets"])
        #expect(workspace.rowLabels(agent: ["widgets"]) == ["widgets"])
        #expect(home.workspaces.last?.rowLabels(agent: nil) == ["Widgets"])
    }

    @Test func computersAreTheirOwnAndMatchNoRealOne() {
        let devices = DemoData.computers.map(\.device)
        #expect(Set(devices).count == devices.count)
        #expect(Set(DemoData.computers.map(\.fingerprint)).count == devices.count)
        #expect(DemoData.computers.allSatisfy { $0.origins.isEmpty })
        #expect(devices.allSatisfy(DemoData.owns))
        #expect(!DemoData.owns("studio"))
    }

    @Test func theWaitingQuestionShowsOnHomeAndInItsChat() throws {
        let home = try #require(DemoData.home(of: DemoData.studio.device))
        let waiting = home.workspaces.flatMap(\.chats).compactMap { chat in chat.summary?.question.map { (chat.pane, $0) } }
        #expect(home.waiting == waiting.count)
        let (pane, question) = try #require(waiting.first)
        let transcript = try #require(DemoData.transcript(pane: pane))
        #expect(transcript.permissions.map(\.id) == [question.id])
    }

    @Test func chatsAndTheTerminalDecode() throws {
        let laptop = try #require(DemoData.home(of: DemoData.laptop.device))
        for pane in laptop.workspaces.flatMap(\.chats).map(\.pane) {
            #expect(!(try #require(DemoData.transcript(pane: pane))).entries.isEmpty)
        }
        let studio = try #require(DemoData.home(of: DemoData.studio.device))
        let terminal = try #require(studio.workspaces.flatMap(\.terminals).first?.panes.first)
        let screen = try #require(DemoData.screen(pane: terminal.pane))
        #expect(screen.pane == terminal.pane)
        #expect(!screen.rows.isEmpty)
    }
}
