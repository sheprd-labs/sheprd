import MoonKit
import SheprdGitHub
import SheprdRemoteCore
import XCTest
@testable import SheprdRemote

/// The Luau Pull Requests tab: what the app publishes to it, and the
/// bundled Luau drawing that without a problem in each state.
@MainActor
final class LuauPullRequestsTests: XCTestCase {
    private let host = GitHubHost("github.com")!

    private func pull(_ number: UInt64, _ title: String, author: String, branch: String,
                      review: GitHubPullRequest.ReviewDecision = .none, draft: Bool = false) -> GitHubSearchPull {
        GitHubSearchPull(host: host, owner: "sheprd-labs", repo: "sheprd", number: number, title: title, isDraft: draft,
                         reviewDecision: review, author: author, updatedAt: .now.addingTimeInterval(-Double(number) * 60),
                         headRefName: branch, headOwner: "sheprd-labs", checks: .init(failing: 1, pending: 2, passed: 3))
    }

    private var lists: PullRequestLists {
        PullRequestLists(
            mine: [pull(2, "Keep the relay's dial keys", author: "octocat", branch: "fix/relay", review: .approved),
                   pull(1, "Docs: GitHub on the phone", author: "octocat", branch: "docs/phone", draft: true)],
            reviewRequested: [pull(3, "Teleport checkpoints", author: "hubot", branch: "feat/teleport")],
            mineTotal: 9
        )
    }

    private var computers: [ComputerWorkspaces] {
        let json = #"{"id":"w1","label":"relay","cwd":"/","chats":[],"terminals":[],"github":{"host":"github.com","owner":"sheprd-labs","repo":"sheprd","branch":"fix/relay"}}"#
        let workspace = try? JSONDecoder().decode(Home.Workspace.self, from: Data(json.utf8))
        return [ComputerWorkspaces(device: "d1", computer: "Studio", workspaces: [workspace].compactMap { $0 })]
    }

    private func snapshot(_ lists: GitHubDirect.Lists, query: String = "", sort: PullRequestSort = .updated) -> PullRequestsSnapshot {
        PullRequestsSnapshot(lists: lists, loading: false, refreshing: false, query: query, sort: sort, device: "iPhone",
                             computers: computers)
    }

    private func ids(_ value: LuauValue, _ section: String) -> [String] {
        value[section]?.array?.compactMap(\.string) ?? []
    }

    func testPublishesTheListsSearchedSortedAndPlaced() throws {
        let read = snapshot(.read(lists))
        XCTAssertEqual(read.value["status"]?.string, "read")
        // Recently updated first: #1 is the newer.
        XCTAssertEqual(ids(read.value, "mine"), [lists.mine[1].id, lists.mine[0].id])
        XCTAssertEqual(ids(read.value, "requested"), [lists.reviewRequested[0].id])
        XCTAssertEqual(read.value["mineTotal"]?.number, 9)

        let relay = try XCTUnwrap(read.value["rows"]?[lists.mine[0].id])
        XCTAssertEqual(relay["review"]?.string, "approved")
        XCTAssertEqual(relay["places"]?.array, [.fields(["computer": .string("Studio"), "device": .string("d1"), "workspace": .string("w1")])])
        XCTAssertEqual(relay["repository"]?.string, "sheprd-labs/sheprd #2")
        XCTAssertEqual(read.value["rows"]?[lists.mine[1].id]?["state"]?.string, "draft")

        // The same search as the SwiftUI tab: totals become what matched.
        let searched = snapshot(.read(lists), query: "teleport")
        XCTAssertEqual(ids(searched.value, "mine"), [])
        XCTAssertEqual(ids(searched.value, "requested"), [lists.reviewRequested[0].id])
        XCTAssertEqual(searched.value["any"]?.bool, true)

        // Status puts the draft last, as `PullRequestLists.sorted` does.
        XCTAssertEqual(ids(snapshot(.read(lists), sort: .status).value, "mine").last, lists.mine[1].id)
    }

    func testWorkspaceMenuOpensTheWorkspaceItNames() {
        let provider = PullRequestsProvider(model: AppModel())
        var opened: [WorkspaceKey] = []
        provider.openWorkspace = { opened.append($0) }
        // By its device and id, whatever the lists say now.
        provider.receive(.fields(["device": .string("d2"), "workspace": .string("w9")]), from: "main")
        provider.receive(.fields(["device": .string(""), "workspace": .string("w9")]), from: "main")
        XCTAssertEqual(opened, [WorkspaceKey(device: "d2", workspace: "w9")])
    }

    func testBundledLuauDrawsEveryState() throws {
        let folder = try XCTUnwrap(Bundle.main.url(forResource: "Luau/pull-requests", withExtension: nil))
        let states: [GitHubDirect.Lists] = [.unread, .noAccount, .failed(last: nil), .failed(last: lists), .read(PullRequestLists()), .read(lists)]
        for state in states {
            let workspace = Workspace(folders: Folders(builtin: folder))
            let published = snapshot(state)
            workspace.attach(Fixed(value: published.value))
            workspace.reload()
            XCTAssertEqual(workspace.problems, [:], "\(state)")
            let tree = try XCTUnwrap(workspace.drawing(of: Workspace.shell).tree, "\(state)")
            XCTAssertEqual(tree.problems, [], "\(state)")
            XCTAssertEqual(tree.kind, .list)
            // Each section draws its rows with `row(i)` as they scroll in, and
            // gives each a dot on the edge index.
            let sections = tree.children.filter { $0.kind == .section && $0.lazyRows != nil }
            for section in sections {
                let key = try XCTUnwrap(section.string("key"))
                let ids = ids(published.value, key)
                let rows = try XCTUnwrap(section.lazyRows)
                XCTAssertEqual(rows.count, ids.count, key)
                XCTAssertEqual(section.props["edge"]?["dots"]?.array?.count, ids.count, key)
                for (index, id) in ids.enumerated() {
                    let row = workspace.part(rows.function, key: .number(Double(index + 1)), of: Workspace.shell)
                    XCTAssertEqual(row.problems, [], "\(key) \(id)")
                    XCTAssertEqual(row.kind, .link)
                    XCTAssertEqual(row.props["route"]?["pull"]?.string, id)
                }
            }
            XCTAssertEqual(sections.count, ["requested", "mine"].filter { !ids(published.value, $0).isEmpty }.count, "\(state)")
        }
    }
}

/// A provider that publishes one value, as `pulls`.
@MainActor
private final class Fixed: Provider {
    let name = "pulls"
    let value: LuauValue

    init(value: LuauValue) { self.value = value }

    func start(_ link: ProviderLink) { link.publish(value) }
}
