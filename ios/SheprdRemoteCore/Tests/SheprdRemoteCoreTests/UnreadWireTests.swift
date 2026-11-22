import Foundation
import Testing
@testable import SheprdRemoteCore

/// The computer marks rows with a reply no one read in its home, which the
/// Rust fixture writes.
struct UnreadWireTests {
    @Test func homeMarksUnreadChatsAndTerminalAgents() throws {
        guard case let .home(home) = try WireTests.event("home") else { Issue.record("not home"); return }
        let workspace = try #require(home.workspaces.first)
        #expect(workspace.chats.first { $0.pane == "p1" }?.unread == true)
        #expect(workspace.chats.first { $0.pane == "p3" }?.unread == nil, "read rows leave it out")
        #expect(workspace.terminals.first?.agent?.unread == true)
    }

    @Test func anOlderComputerLeavesItOut() throws {
        let json = #"{"pane":"p","tab":"t","title":"x","status":"done"}"#
        let row = try JSONDecoder().decode(Home.ChatRow.self, from: Data(json.utf8))
        #expect(row.unread == nil)
    }
}
