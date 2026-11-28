import SwiftUI

// The worktrees of one repository, opened by tapping the branch row at the
// top of a workspace. A: grouped by computer, one line of counts. B: sorted
// by activity, the last thing an agent said. C: a branch tree off main with
// ahead/behind and changes. D: a sheet over the workspace, to hop between them.

private enum TreeMark {
    case pr(Int, Color), uncommitted, openPR

    @ViewBuilder var view: some View {
        switch self {
        case let .pr(number, color):
            Text(verbatim: "#\(number)").font(.caption.weight(.semibold).monospacedDigit())
                .foregroundStyle(color).padding(.horizontal, 7).padding(.vertical, 2)
                .background(color.opacity(0.15), in: .capsule).fixedSize()
        case .openPR:
            Text("+PR").font(.caption.weight(.semibold)).foregroundStyle(.yellow)
                .padding(.horizontal, 7).padding(.vertical, 2)
                .background(Color.yellow.opacity(0.15), in: .capsule).fixedSize()
        case .uncommitted:
            Image(systemName: "pencil").font(.caption.weight(.semibold)).foregroundStyle(.yellow)
                .frame(width: 24, height: 22).background(Color.yellow.opacity(0.15), in: .rect(cornerRadius: 6))
        }
    }
}

private struct Tree: Identifiable {
    var id: String { branch }
    var branch: String
    var label: String?
    var computer: String
    var agents: Int
    var terminals: Int
    var status: Status
    var agent: Agent?
    var said: String
    var time: String
    var ahead = 0
    var behind = 0
    var changed = 0
    var mark: TreeMark?
    var current = false
    var isMain: Bool { branch == "main" }
}

private let trees: [Tree] = [
    Tree(branch: "feat/msgvault", computer: "studio", agents: 10, terminals: 2, status: .working, agent: .codex,
         said: "Choose Mail → Connect live Maildir to share a local tree with Mutt", time: "now",
         ahead: 14, behind: 2, changed: 3, mark: .uncommitted, current: true),
    Tree(branch: "feat/imap-fast-sync", computer: "studio", agents: 1, terminals: 0, status: .done, agent: .codex,
         said: "Committed fast IMAP sync as 48c1431. The worktree is clean.", time: "11h",
         ahead: 3, mark: .openPR),
    Tree(branch: "feat/imap-idle", label: "Persistent IMAP sessions", computer: "studio", agents: 1, terminals: 1,
         status: .needsYou, agent: .claude, said: "Should IDLE reconnect on wake or wait for the next poll?",
         time: "10h", ahead: 5, behind: 1, mark: .pr(212, .green)),
    Tree(branch: "fastmail", computer: "studio", agents: 1, terminals: 1, status: .unread, agent: .claude,
         said: "JMAP push works; the pencil is the token refresh fix", time: "2h",
         ahead: 2, changed: 1, mark: .uncommitted),
    Tree(branch: "main", computer: "studio", agents: 0, terminals: 1, status: .idle, agent: nil,
         said: "zsh — ~/code/moonmail", time: "1d"),
    Tree(branch: "gmail-xgm", label: "Gmail IMAP extensions", computer: "macbook", agents: 1, terminals: 0,
         status: .done, agent: .claude, said: "X-GM-LABELS map onto folders; merged", time: "Mon",
         ahead: 0, mark: .pr(208, .purple)),
    Tree(branch: "main", computer: "macbook", agents: 0, terminals: 1, status: .idle, agent: nil,
         said: "zsh — ~/src/moonmail", time: "Sun", behind: 9),
]

private func counts(_ tree: Tree) -> String {
    var parts: [String] = []
    if tree.agents > 0 { parts.append(tree.agents == 1 ? "1 agent" : "\(tree.agents) agents") }
    if tree.terminals > 0 { parts.append(tree.terminals == 1 ? "1 terminal" : "\(tree.terminals) terminals") }
    return parts.isEmpty ? "No tabs" : parts.joined(separator: " · ")
}

private struct Dot: View {
    var status: Status
    var body: some View {
        switch status {
        case .idle: Circle().strokeBorder(.secondary, lineWidth: 1.5).frame(width: 10, height: 10)
        case .working: Circle().fill(.teal).frame(width: 10, height: 10)
        case .needsYou: Circle().fill(.orange).frame(width: 10, height: 10)
        case .unread: Circle().fill(.blue).frame(width: 10, height: 10)
        case .done: Circle().strokeBorder(.green, lineWidth: 1.5).frame(width: 10, height: 10)
        }
    }
}

private struct BranchName: View {
    var tree: Tree
    var body: some View {
        Text(tree.label ?? tree.branch)
            .font(tree.label == nil ? .body.monospaced() : .body)
            .lineLimit(1)
    }
}

/// The repository card heading A and B.
private struct RepoHeader: View {
    var body: some View {
        HStack(spacing: 12) {
            Text("M").font(.headline).foregroundStyle(.white).frame(width: 36, height: 36)
                .background(Color.red.gradient, in: .rect(cornerRadius: 9))
            VStack(alignment: .leading, spacing: 2) {
                Text("moonmail").font(.headline)
                Text("github.com/penso/moonmail").font(.subheadline).foregroundStyle(.secondary)
            }
        }
    }
}

private struct NewWorktreeRow: View {
    var body: some View {
        Label("New worktree", systemImage: "plus").foregroundStyle(.blue)
    }
}

private struct WorktreesShell<Content: View>: View {
    var title = "moonmail"
    @ViewBuilder var content: Content
    var body: some View {
        NavigationStack {
            List { content }
                .navigationTitle(title)
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .topBarLeading) { Image(systemName: "chevron.left") }
                    ToolbarItem(placement: .topBarTrailing) { Image(systemName: "plus") }
                }
        }
    }
}

// A: grouped by computer.
struct WorktreesByComputer: View {
    var body: some View {
        WorktreesShell {
            Section { RepoHeader() } footer: { Text("7 worktrees on 2 computers") }
            ForEach(["studio", "macbook"], id: \.self) { computer in
                Section {
                    ForEach(trees.filter { $0.computer == computer }.sorted { $0.isMain && !$1.isMain }) { tree in
                        NavigationLink(value: tree.id) {
                            HStack(spacing: 12) {
                                Dot(status: tree.status)
                                VStack(alignment: .leading, spacing: 2) {
                                    BranchName(tree: tree)
                                    Text(counts(tree)).font(.subheadline).foregroundStyle(.secondary)
                                }
                                Spacer()
                                tree.mark?.view
                                if tree.current {
                                    Image(systemName: "checkmark").font(.footnote.weight(.bold)).foregroundStyle(.blue)
                                }
                            }
                        }
                    }
                } header: {
                    Label(computer, systemImage: "laptopcomputer")
                }
            }
        }
    }
}

// B: by activity, with what the last agent said.
struct WorktreesActivity: View {
    var body: some View {
        WorktreesShell {
            Section {
                ForEach(trees.filter { $0.computer == "studio" || !$0.isMain }) { tree in
                    NavigationLink(value: tree.id) {
                        HStack(alignment: .top, spacing: 12) {
                            if let agent = tree.agent { AgentIcon(agent: agent, size: 30) } else { AgentIcon(agent: .terminal, size: 30) }
                            VStack(alignment: .leading, spacing: 3) {
                                HStack(alignment: .firstTextBaseline) {
                                    BranchName(tree: tree).fontWeight(tree.current ? .semibold : .regular)
                                    Spacer()
                                    Text(tree.time).font(.footnote).foregroundStyle(.secondary)
                                }
                                Text(tree.said).font(.subheadline).foregroundStyle(.secondary).lineLimit(2)
                                HStack(spacing: 6) {
                                    Dot(status: tree.status)
                                    Text("\(tree.computer) · \(counts(tree))").font(.caption).foregroundStyle(.secondary)
                                    Spacer()
                                    tree.mark?.view
                                }
                            }
                        }
                        .padding(.vertical, 2)
                    }
                    .listRowBackground(tree.current ? Color.blue.opacity(0.08) : nil)
                }
            } header: {
                Text("6 worktrees · latest first")
            }
        }
    }
}

// C: a branch tree off main.
struct WorktreesTree: View {
    var body: some View {
        WorktreesShell {
            Section {
                HStack(spacing: 12) {
                    Image(systemName: "arrow.triangle.branch").foregroundStyle(.blue).frame(width: 28)
                    VStack(alignment: .leading, spacing: 2) {
                        Text("main").font(.body.monospaced().weight(.semibold))
                        Text("studio · ~/code/moonmail").font(.subheadline).foregroundStyle(.secondary)
                    }
                    Spacer()
                    Text("e41a0c2").font(.caption.monospaced()).foregroundStyle(.secondary)
                }
            }
            Section {
                let branches = trees.filter { !$0.isMain }
                ForEach(Array(branches.enumerated()), id: \.element.id) { index, tree in
                    NavigationLink(value: tree.id) {
                        HStack(spacing: 10) {
                            Text(index == branches.count - 1 ? "└" : "├").font(.title3.monospaced())
                                .foregroundStyle(.tertiary).frame(width: 18)
                            VStack(alignment: .leading, spacing: 3) {
                                HStack(spacing: 6) {
                                    BranchName(tree: tree).fontWeight(tree.current ? .semibold : .regular)
                                    if tree.current {
                                        Text("here").font(.caption2.weight(.bold)).foregroundStyle(.blue)
                                            .padding(.horizontal, 5).padding(.vertical, 1)
                                            .background(Color.blue.opacity(0.15), in: .capsule)
                                    }
                                }
                                HStack(spacing: 10) {
                                    Label("\(tree.ahead)", systemImage: "arrow.up").foregroundStyle(.green)
                                    Label("\(tree.behind)", systemImage: "arrow.down")
                                        .foregroundStyle(tree.behind > 0 ? .orange : .secondary)
                                    if tree.changed > 0 {
                                        Label("\(tree.changed)", systemImage: "plusminus").foregroundStyle(.yellow)
                                    }
                                    Text(tree.computer).foregroundStyle(.secondary)
                                }
                                .font(.caption.monospacedDigit()).labelStyle(TightLabel())
                            }
                            Spacer()
                            tree.mark?.view
                        }
                    }
                }
            } header: {
                Text("Worktrees off main")
            }
            Section { NewWorktreeRow() }
        }
    }
}

private struct TightLabel: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 2) { configuration.icon; configuration.title }
    }
}

// D: a sheet over the workspace, to hop to a sibling.
struct WorktreesSheet: View {
    var body: some View {
        NavigationStack {
            List {
                Section {
                    HStack(spacing: 12) {
                        Image(systemName: "arrow.triangle.branch").foregroundStyle(.blue)
                            .frame(width: 32, height: 32).background(Color.blue.opacity(0.15), in: .rect(cornerRadius: 8))
                        VStack(alignment: .leading) {
                            Text("moonmail")
                            Text("feat/msgvault").font(.subheadline.monospaced()).foregroundStyle(.secondary)
                        }
                        Spacer()
                        Text("7 worktrees").font(.subheadline).foregroundStyle(.secondary)
                        Image(systemName: "chevron.up.chevron.down").font(.footnote).foregroundStyle(.tertiary)
                    }
                }
                Section("12 tabs") {
                    ForEach(0..<4) { _ in
                        HStack { AgentIcon(agent: .codex); Text("Live Maildir account for Mutt").lineLimit(1) }
                    }
                }
            }
            .navigationTitle("moonmail").navigationBarTitleDisplayMode(.inline)
            .sheet(isPresented: .constant(true)) {
                NavigationStack {
                    List {
                        ForEach(trees) { tree in
                            HStack(spacing: 12) {
                                Dot(status: tree.status)
                                VStack(alignment: .leading, spacing: 1) {
                                    BranchName(tree: tree)
                                    Text("\(tree.computer) · \(counts(tree))").font(.caption).foregroundStyle(.secondary)
                                }
                                Spacer()
                                tree.mark?.view
                                if tree.current { Image(systemName: "checkmark").foregroundStyle(.blue).fontWeight(.semibold) }
                            }
                        }
                        NewWorktreeRow()
                    }
                    .navigationTitle("Worktrees").navigationBarTitleDisplayMode(.inline)
                    .toolbar { ToolbarItem(placement: .topBarTrailing) { Image(systemName: "xmark") } }
                }
                .presentationDetents([.fraction(0.62)])
            }
        }
    }
}
