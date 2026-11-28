#if DEBUG
import MoonKit
import SheprdRemoteCore
import XCTest
@testable import SheprdRemote

/// `sheprd phone luau`: an agent's files over a Luau screen's bundled
/// ones, what loading them finds, and going back to the bundle.
@MainActor
final class LuauLiveTests: XCTestCase {
    private let screen = "pull-requests"

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: LuauLive.overlay(screen))
    }

    private func bundledMain() throws -> String {
        let folder = try XCTUnwrap(LuauLive.bundled(screen))
        return try String(contentsOf: folder.appending(path: "main.luau"), encoding: .utf8)
    }

    func testAPushThatBreaksSaysWhereAndTheBundleComesBack() throws {
        let broken = LuauLive.shared.apply(screen: screen, files: ["main.luau": "return mod {"])
        XCTAssertFalse(broken.shown, "the tab is not open in a test")
        XCTAssertEqual(broken.problems.count, 1)
        XCTAssertTrue(broken.problems[0].hasPrefix("main.luau:"), broken.problems[0])
        XCTAssertFalse(broken.problems[0].hasPrefix("main.luau: main.luau"), "the file is named once")

        let fixed = LuauLive.shared.apply(screen: screen, files: ["main.luau": try bundledMain()])
        XCTAssertEqual(fixed.problems, [])

        let reset = LuauLive.shared.apply(screen: screen, files: [:])
        XCTAssertEqual(reset.problems, [])
        XCTAssertFalse(FileManager.default.fileExists(atPath: LuauLive.overlay(screen).path))
    }

    func testAnOpenScreenReloadsWithThePush() throws {
        let bundled = try XCTUnwrap(LuauLive.bundled(screen))
        let workspace = Workspace(folders: LuauLive.folders(screen, bundled: bundled))
        workspace.reload()
        LuauLive.shared.register(workspace, screen: screen)
        let changed = try bundledMain().replacingOccurrences(of: "\"Mine\"", with: "\"Yours\"")

        let report = LuauLive.shared.apply(screen: screen, files: ["main.luau": changed])

        XCTAssertTrue(report.shown)
        XCTAssertEqual(report.problems, [])
        XCTAssertEqual(workspace.problems, [:])
    }

    func testAScreenTheAppDoesNotHaveIsSaidSo() {
        let report = LuauLive.shared.apply(screen: "nope", files: ["main.luau": "return 1"])
        XCTAssertEqual(report.problems, ["this app has no Luau screen named nope"])
    }

    func testAPathOutOfTheFolderIsNotWritten() throws {
        _ = LuauLive.shared.apply(screen: screen, files: ["../escape.luau": "x", "main.luau": try bundledMain()])
        let parent = LuauLive.overlay(screen).deletingLastPathComponent()
        XCTAssertFalse(FileManager.default.fileExists(atPath: parent.appending(path: "escape.luau").path))
    }

    /// A snapshot goes to a computer only when nothing else's shows.
    func testASnapshotShowsNothingOfAnotherComputer() {
        let mine = "a", both = ["a", "b"]
        XCTAssertTrue(AgentControl.showsOnly(.chat(pane: "p"), device: mine, paired: both))
        XCTAssertTrue(AgentControl.showsOnly(.workspace("w"), device: mine, paired: both))
        XCTAssertFalse(AgentControl.showsOnly(.pullRequests, device: mine, paired: both), "a tab lists both")
        XCTAssertTrue(AgentControl.showsOnly(.pullRequests, device: mine, paired: [mine]))
        XCTAssertFalse(AgentControl.showsOnly(nil, device: mine, paired: [mine]), "another's chat, or the lock")
    }
}
#endif
