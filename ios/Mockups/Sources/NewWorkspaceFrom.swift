import SwiftUI

// The new workspace form with the desktop's worktree sources: what it
// starts from (a new branch, a local branch, a pull request, an issue, an
// existing worktree) and what it opens in. Pull: a pull request picked,
// then its list. Shell: an existing worktree opening in a terminal. Menu:
// the start choices.

private struct Tile: Identifiable {
    var name: String
    var symbol: String
    var tint: Color
    var id: String { name }
}

private let tiles: [Tile] = [
    Tile(name: "Antigravity", symbol: "triangle", tint: .gray),
    Tile(name: "Claude", symbol: "sparkle", tint: Agent.claude.tint),
    Tile(name: "Codex", symbol: "hexagon.fill", tint: Agent.codex.tint),
    Tile(name: "Copilot", symbol: "person.fill", tint: .purple),
    Tile(name: "Gemini", symbol: "diamond.fill", tint: Agent.gemini.tint),
    Tile(name: "OpenCode", symbol: "square", tint: .gray),
]

private let starts: [(String, String)] = [
    ("This folder", "folder"), ("New branch", "plus"), ("Branch", "arrow.triangle.branch"),
    ("Pull request", "arrow.triangle.pull"), ("Issue", "smallcircle.filled.circle"),
    ("Existing worktree", "square.stack.3d.up"),
]

struct NewWorkspaceFrom: View {
    var variant: String
    var state: Int

    var body: some View {
        NavigationStack {
            switch (variant, state) {
            case ("Pull", 1): PullList()
            default: form
            }
        }
    }

    private var shell: Bool { variant == "Shell" }

    private var form: some View {
        Form {
            Section("Computer") {
                LabeledContent("Computer") {
                    HStack(spacing: 4) { Text("studio"); Image(systemName: "chevron.up.chevron.down").font(.caption) }
                        .foregroundStyle(.tint)
                }
            }
            Section {
                NavigationLink {} label: {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("herdr-gpui").font(.headline)
                        Text("/Users/penso/code/herdr-gpui").font(.caption.monospaced()).foregroundStyle(.secondary)
                    }
                }
                LabeledContent("Start from") {
                    HStack(spacing: 4) {
                        Text(shell ? "Existing worktree" : "Pull request")
                        Image(systemName: "chevron.up.chevron.down").font(.caption)
                    }
                    .foregroundStyle(.tint)
                }
                if variant == "Menu" {
                    ForEach(starts, id: \.0) { start in
                        Label(start.0, systemImage: start.1)
                            .foregroundStyle(start.0 == "Pull request" ? AnyShapeStyle(.tint) : AnyShapeStyle(.primary))
                    }
                }
                NavigationLink {} label: {
                    if shell {
                        Text("fix/flaky-network").font(.body.monospaced())
                    } else {
                        Text("#1702 Fix flaky network tests").lineLimit(1)
                    }
                }
                Text(shell ? "Title (optional)" : "#1702 Fix flaky network tests").foregroundStyle(.tertiary).lineLimit(1)
            } header: {
                Text("Project")
            } footer: {
                Text(shell ? "Opening adds the worktree to Sheprd; nothing is created." : "The computer fetches the pull request's branch first.")
            }
            Section("Opens in") {
                Picker("Opens in", selection: .constant(shell ? 1 : 0)) {
                    Text("Chat").tag(0)
                    Text("Terminal").tag(1)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
            }
            if !shell {
                Section("Agent") {
                    LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 10), count: 3), spacing: 10) {
                        ForEach(tiles) { tile in
                            VStack(spacing: 6) {
                                Image(systemName: tile.symbol).font(.title3).foregroundStyle(.white)
                                    .frame(width: 34, height: 34)
                                    .background(tile.tint.gradient, in: .rect(cornerRadius: 9))
                                Text(tile.name).font(.caption.weight(.medium))
                            }
                            .frame(maxWidth: .infinity).padding(.vertical, 10)
                            .background(tile.name == "Claude" ? AnyShapeStyle(.tint.opacity(0.16)) : AnyShapeStyle(.fill.quinary), in: .rect(cornerRadius: 14))
                            .overlay(RoundedRectangle(cornerRadius: 14).strokeBorder(tile.name == "Claude" ? AnyShapeStyle(.tint) : AnyShapeStyle(.clear), lineWidth: 1.5))
                        }
                    }
                    .padding(.vertical, 4)
                }
            }
        }
        .navigationTitle("New Workspace")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .cancellationAction) { Button("Cancel") {} }
            ToolbarItem(placement: .confirmationAction) { Button("Create") {} }
        }
    }
}

private struct PullList: View {
    private let pulls: [(Int, String, String, String, Bool)] = [
        (1702, "Fix flaky network tests", "penso", "fix/flaky-network", false),
        (1699, "Multiplex ACP chats over one socket", "penso", "feat/acp-multiplex", false),
        (1716, "Submit TestFlight builds for beta review", "ana", "ci/beta-review", true),
        (1688, "List San Francisco and Amsterdam relays", "bo", "relay/public-list", false),
        (1671, "Group tabs by worktree in the iPad sidebar", "cy", "ipad/worktree-tabs", false),
    ]

    var body: some View {
        List(pulls, id: \.0) { pull in
            HStack {
                VStack(alignment: .leading, spacing: 3) {
                    HStack(alignment: .firstTextBaseline, spacing: 6) {
                        Text("#\(pull.0)").font(.subheadline.monospacedDigit()).foregroundStyle(.secondary)
                        Text(pull.1).lineLimit(2)
                    }
                    HStack(spacing: 6) {
                        Text(pull.2)
                        Text(pull.3).font(.caption.monospaced())
                        if pull.4 { Text("draft").padding(.horizontal, 5).background(.fill.tertiary, in: .capsule) }
                    }
                    .font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                if pull.0 == 1702 { Image(systemName: "checkmark").foregroundStyle(.tint) }
            }
        }
        .searchable(text: .constant(""), placement: .navigationBarDrawer(displayMode: .always), prompt: "Number, title, author, branch")
        .navigationTitle("Pull request")
        .navigationBarTitleDisplayMode(.inline)
    }
}
