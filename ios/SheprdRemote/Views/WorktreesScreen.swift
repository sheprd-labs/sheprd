import SheprdRemoteCore
import SwiftUI

/// Every worktree of a workspace's repository, opened from the project row
/// at the top of `WorkspaceScreen`: the repository, then each computer
/// holding a checkout of it, its own computer first, with the rows the
/// Workspaces tab draws. The workspace it came from carries a check; the
/// others open their own workspace screen.
struct WorktreesScreen: View {
    @Environment(AppModel.self) private var model
    let key: WorkspaceKey

    var body: some View {
        let machines = model.fleet.machines
        if let workspace = machines.first(where: { $0.id == key.device })?.home?.workspaces.first(where: { $0.id == key.workspace }) {
            let homes = machines.compactMap { machine in machine.home.map { (device: machine.id, home: $0) } }
            content(workspace, workspace.checkouts(on: key.device, among: homes))
        } else {
            ContentUnavailableView("This workspace is gone", systemImage: "folder.badge.questionmark",
                                   description: Text("It was closed in Sheprd, or its computer is not connected."))
        }
    }

    private func content(_ workspace: Home.Workspace, _ checkouts: [ComputerCheckouts]) -> some View {
        let name = workspace.repositoryName
        return List {
            Section {
                RepositoryCard(name: name, github: workspace.github)
            } footer: {
                Text(footer(checkouts))
            }
            ForEach(checkouts) { computer in
                Section {
                    ForEach(computer.workspaces) { other in
                        row(other, on: computer.device, repositoryLabel: name)
                    }
                } header: {
                    ComputerHeader(device: computer.device)
                }
            }
        }
        .navigationTitle(name)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if let own = workspace.ownCheckout(among: checkouts.first?.workspaces ?? []) {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("New Worktree", systemImage: "plus") {
                        model.creating = .workspace(device: key.device, folder: own.cwd, session: own.session)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func row(_ workspace: Home.Workspace, on device: String, repositoryLabel: String) -> some View {
        let target = WorkspaceKey(device: device, workspace: workspace.id)
        if target == key {
            // Where this list was opened from: Back returns there.
            HStack(spacing: 8) {
                WorkspaceRow(workspace: workspace, repositoryLabel: repositoryLabel)
                Image(systemName: "checkmark").font(.footnote.weight(.bold)).foregroundStyle(.tint)
            }
            .accessibilityElement(children: .combine)
            .accessibilityAddTraits(.isSelected)
        } else {
            NavigationLink {
                WorkspaceScreen(key: target).makesRoomForConnectionToast()
            } label: {
                WorkspaceRow(workspace: workspace, repositoryLabel: repositoryLabel)
            }
        }
    }

    private func footer(_ checkouts: [ComputerCheckouts]) -> String {
        let count = checkouts.reduce(0) { $0 + $1.workspaces.count }
        let worktrees = count == 1 ? String(localized: "1 worktree") : String(localized: "\(count) worktrees")
        return checkouts.count > 1 ? String(localized: "\(worktrees) on \(checkouts.count) computers") : worktrees
    }
}

/// The repository: its name on its tile, and where it is pushed.
private struct RepositoryCard: View {
    var name: String
    var github: Home.GitHubRepository?

    var body: some View {
        HStack(spacing: 12) {
            RepositoryAvatar(label: name, size: 36)
            VStack(alignment: .leading, spacing: 2) {
                Text(name).font(.headline).lineLimit(1)
                if let github {
                    Text(verbatim: "\(github.host)/\(github.owner)/\(github.repo)")
                        .font(.subheadline).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
                }
            }
        }
        .accessibilityElement(children: .combine)
    }
}

/// A computer above its checkouts, and whether the phone reaches it.
private struct ComputerHeader: View {
    @Environment(AppModel.self) private var model
    var device: String

    var body: some View {
        if let machine = model.fleet.machines.first(where: { $0.id == device }) {
            HStack(spacing: 6) {
                Image(systemName: machine.computer.via == nil ? "laptopcomputer" : "server.rack")
                Text(model.displayName(machine.computer)).lineLimit(1)
                Spacer()
                MachineStatus(machine: machine)
            }
            .textCase(nil)
        }
    }
}
