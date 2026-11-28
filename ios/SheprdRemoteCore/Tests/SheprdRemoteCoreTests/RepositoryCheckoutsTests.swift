import Foundation
import Testing
@testable import SheprdRemoteCore

/// A workspace's worktree list finds every checkout of its repository, on
/// its own computer and on the others.
struct RepositoryCheckoutsTests {
    private static let moonmail = Home.GitHubRepository(host: "github.com", owner: "penso", repo: "moonmail")

    private static func workspace(_ id: String, key: String?, linked: Bool = false, github: Home.GitHubRepository? = nil) -> Home.Workspace {
        var workspace = RepositoriesTests.workspace(id, worktree: key.map { .init(key: $0, label: "moonmail", linked: linked) })
        workspace.github = github
        return workspace
    }

    @Test func listsTheSameComputersWorktreesFirstThenOtherComputersByRemote() {
        let current = Self.workspace("vault", key: "/studio/.git", linked: true, github: Self.moonmail)
        let studio = RepositoriesTests.home([
            Self.workspace("other", key: "/studio/other/.git"),
            current,
            Self.workspace("main", key: "/studio/.git", github: Self.moonmail),
            // Same repository, no remote known: the worktree key still joins it.
            Self.workspace("fast", key: "/studio/.git", linked: true),
        ])
        let macbook = RepositoriesTests.home([
            Self.workspace("mb-main", key: "/mb/.git", github: .init(host: "GitHub.com", owner: "Penso", repo: "MoonMail")),
            Self.workspace("mb-wt", key: "/mb/.git", linked: true),
            Self.workspace("unrelated", key: "/mb/x/.git", github: .init(host: "github.com", owner: "penso", repo: "moonmap")),
        ])
        let empty = RepositoriesTests.home([Self.workspace("lonely", key: "/e/.git")])

        let checkouts = current.checkouts(on: "studio", among: [("macbook", macbook), ("empty", empty), ("studio", studio)])
        #expect(checkouts.map(\.device) == ["studio", "macbook"])
        #expect(checkouts[0].workspaces.map(\.id) == ["main", "vault", "fast"])
        #expect(checkouts[1].workspaces.map(\.id) == ["mb-main", "mb-wt"])
    }

    @Test func aWorkspaceOutsideAnyRepositoryHasOnlyItself() {
        let alone = RepositoriesTests.workspace("scratch")
        let home = RepositoriesTests.home([alone, RepositoriesTests.workspace("other")])
        let checkouts = alone.checkouts(on: "c", among: [("c", home)])
        #expect(checkouts == [ComputerCheckouts(device: "c", workspaces: [alone])])
    }

    @Test func keepsTheWorkspaceWhenItsComputersHomeIsMissing() {
        let current = Self.workspace("vault", key: "/studio/.git", github: Self.moonmail)
        let macbook = RepositoriesTests.home([Self.workspace("mb", key: "/mb/.git", github: Self.moonmail)])
        let checkouts = current.checkouts(on: "studio", among: [("macbook", macbook)])
        #expect(checkouts.map(\.device) == ["studio", "macbook"])
        #expect(checkouts[0].workspaces.map(\.id) == ["vault"])
    }

    @Test func branchesANewWorktreeFromTheOwnCheckoutOfItsRepositoryAndSession() {
        func own(_ id: String, key: String, session: String) -> Home.Workspace {
            var workspace = Self.workspace(id, key: key, github: Self.moonmail)
            workspace.session = session
            return workspace
        }
        var current = Self.workspace("vault", key: "/a/.git", linked: true, github: Self.moonmail)
        current.session = "pilots"
        let others = [
            own("clone", key: "/b/.git", session: "pilots"),
            own("coding", key: "/a/.git", session: "coding"),
            own("pilots", key: "/a/.git", session: "pilots"),
        ]
        #expect(current.ownCheckout(among: others)?.id == "pilots")
        current.session = "other"
        #expect(current.ownCheckout(among: others)?.id == "coding")
        #expect(others[1].ownCheckout(among: others)?.id == "coding")
        current.worktree = nil
        #expect(current.ownCheckout(among: others) == nil)
    }
}
