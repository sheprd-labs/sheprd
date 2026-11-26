import Testing
@testable import SheprdRemoteCore

/// A permission to run a command is shown as code, not as a heading.
struct PermissionCommandTests {
    @Test func readsAOneLineCommand() throws {
        let command = try #require(PermissionCommand(title: "`cd /tmp && rm -f a.zip`"))
        #expect(command.text == "cd /tmp && rm -f a.zip")
        #expect(command.moreLines == 0)
    }

    @Test func keepsBackticksInsideTheCommand() throws {
        let command = try #require(PermissionCommand(title: "`echo `date``"))
        #expect(command.text == "echo `date`")
    }

    @Test func countsTheLinesTheTitleLeavesOut() throws {
        #expect(PermissionCommand(title: "`make` (+1 more line)")?.moreLines == 1)
        #expect(PermissionCommand(title: "`make` (+4 more lines)")?.moreLines == 4)
    }

    @Test func otherTitlesAreNotCommands() {
        #expect(PermissionCommand(title: "Edit src/main.rs") == nil)
        #expect(PermissionCommand(title: "Ready to code?") == nil)
        #expect(PermissionCommand(title: "``") == nil)
        #expect(PermissionCommand(title: "`make` then more") == nil)
        #expect(PermissionCommand(title: "`make` (+1 more lines)") == nil)
    }
}
