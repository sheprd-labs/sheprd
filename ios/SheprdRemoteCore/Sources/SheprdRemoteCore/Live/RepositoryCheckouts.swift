import Foundation

/// The checkouts of one repository on one computer, which a workspace's
/// worktree list shows under that computer's name.
public struct ComputerCheckouts: Hashable, Sendable, Identifiable {
    public var device: String
    /// Its own checkouts first, then its linked worktrees, in Sheprd's order.
    public var workspaces: [Home.Workspace]

    public var id: String { device }
}

extension Home.Workspace {
    /// Every checkout of this workspace's repository: those sharing its
    /// worktree key on its own computer, and on any computer those pushing to
    /// the same GitHub repository, as worktree keys are paths only one
    /// computer knows. Its own computer comes first, then the others in `homes`'
    /// order; computers holding none are left out. A workspace whose
    /// repository Sheprd does not know has only itself.
    public func checkouts(on device: String, among homes: [(device: String, home: Home)]) -> [ComputerCheckouts] {
        let ordered = homes.filter { $0.device == device } + homes.filter { $0.device != device }
        let found = ordered.compactMap { entry -> ComputerCheckouts? in
            let mine = entry.device == device
            let members = entry.home.repositories.filter { repository in
                repository.workspaces.contains { (mine && $0.id == id) || sameRemote(as: $0) }
            }.flatMap(\.workspaces)
            return members.isEmpty ? nil : ComputerCheckouts(device: entry.device, workspaces: members)
        }
        return found.contains { $0.device == device } ? found : [ComputerCheckouts(device: device, workspaces: [self])] + found
    }

    /// The checkout a new worktree of this workspace branches from: among
    /// `workspaces` on its own computer, the own checkout of its repository
    /// (its worktree key), in its Herdr session when one is there; nil when
    /// Sheprd names no repository for it or none is open.
    public func ownCheckout(among workspaces: [Home.Workspace]) -> Home.Workspace? {
        if isOwnCheckout, worktree != nil { return self }
        guard let key = worktree?.key else { return nil }
        let own = workspaces.filter { $0.isOwnCheckout && $0.worktree?.key == key }
        return own.first { $0.session == session } ?? own.first
    }

    /// Both push to one GitHub repository, whatever the letters' case.
    private func sameRemote(as other: Home.Workspace) -> Bool {
        guard let mine = github, let theirs = other.github else { return false }
        return mine.host.lowercased() == theirs.host.lowercased()
            && mine.owner.lowercased() == theirs.owner.lowercased()
            && mine.repo.lowercased() == theirs.repo.lowercased()
    }
}
