import Foundation
import Testing
@testable import SheprdRemoteCore

/// The links an agent writes to a screen of its own computer.
struct ScreenLinkTests {
    private func screen(_ text: String) throws -> AgentScreen? {
        ScreenLink.screen(try #require(URL(string: text)))
    }

    @Test func readsEachKind() throws {
        #expect(try screen("sheprd://chat/wF9:p1") == .chat(pane: "wF9:p1"))
        #expect(try screen("sheprd://term/wF9:p2") == .term(pane: "wF9:p2"))
        #expect(try screen("sheprd://workspace/wF9") == .workspace("wF9"))
        #expect(try screen("sheprd://changes/wF9") == .changes(workspace: "wF9"))
        #expect(try screen("SHEPRD://Workspace/wF9") == .workspace("wF9"))
        #expect(try screen("sheprd://term/w%201") == .term(pane: "w 1"))
    }

    @Test func refusesAnythingElse() throws {
        for text in [
            "https://x.dev/workspace/wF9",
            "herdr-remote://chat?id=1&pane=p",
            "sheprd://settings/push",
            "sheprd://new_workspace/x",
            "sheprd://workspace",
            "sheprd://workspace/",
            "sheprd://workspace/a/b",
        ] {
            #expect(try screen(text) == nil, "\(text)")
        }
    }

    @Test func namesACoreTerminalAsTheDesktopDoes() {
        // `t7` is 74 37 in hex; 0a1b2c3d is the boot's tag.
        #expect(ScreenLink.pane("core:0a1b2c3d-7437", isNamed: "core:t7"))
        #expect(ScreenLink.pane("core:0a1b2c3d-7437", isNamed: "core:0a1b2c3d-7437"))
        #expect(ScreenLink.pane("wF9:p1", isNamed: "wF9:p1"))
        #expect(!ScreenLink.pane("core:0a1b2c3d-7438", isNamed: "core:t7"))
        #expect(!ScreenLink.pane("core:0a1b2c3d-7437", isNamed: "core:"))
        #expect(!ScreenLink.pane("core:zz-7437", isNamed: "core:t7"))
        #expect(!ScreenLink.pane("wF9:7437", isNamed: "core:t7"))
    }
}
