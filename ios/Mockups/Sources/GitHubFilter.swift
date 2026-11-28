import SwiftUI

// A filter menu on the PRs tab, beside its sort menu, as Home has one.
// A: the menu drawn open (state 0); the list filtered to Merged and
// Closed, its icon filled (state 1).

/// The app's four tabs, PRs in front.
private struct PRTabs<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        TabView(selection: .constant(2)) {
            Tab("Home", systemImage: "house", value: 0) { Text("Home") }
            Tab("Workspaces", systemImage: "square.stack.3d.up", value: 1) { Text("Workspaces") }
            Tab("PRs", systemImage: "arrow.triangle.pull", value: 2) { content }
            Tab("Settings", systemImage: "gearshape", value: 3) { Text("Settings") }
        }
    }
}

private struct FilterRow: View {
    var pull: GHPull
    var showsAuthor: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack(spacing: 6) {
                Image(systemName: pull.state.symbol).font(.caption.weight(.semibold)).foregroundStyle(pull.state.color)
                Text(verbatim: "\(pull.repo) #\(pull.number)").font(.caption.monospaced())
                    .foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
                Spacer(minLength: 4)
                Text(pull.updated).font(.caption).foregroundStyle(.secondary)
            }
            Text(pull.title).lineLimit(2)
            HStack(spacing: 8) {
                if showsAuthor {
                    GHAvatar(person: pull.author, size: 16)
                    Text(pull.author.login).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                } else {
                    Text(pull.state.label).font(.caption).foregroundStyle(.secondary)
                }
                Spacer(minLength: 4)
                GHCheckCounts(failing: pull.failing, pending: pull.pending, passed: pull.passed)
            }
        }
        .padding(.vertical, 2)
    }
}

struct PRFilterMenu: View {
    var state: Int
    private var filtered: Bool { state == 1 }

    private static let requestedClosed: [GHPull] = [
        GHPull(repo: GH.repo, number: 1329, title: "Teleport: copy the worktree's stash", state: .merged,
               review: .approved, author: GH.hubot, branch: "teleport-stash", updated: "1d", passed: 14),
        GHPull(repo: "sheprd-labs/sheprd-apns", number: 51, title: "Drop the sandbox gateway", state: .closed,
               review: nil, author: GH.monalisa, branch: "no-sandbox", updated: "3d", failing: 1, passed: 5),
    ]

    private static let mineClosed: [GHPull] = [
        GHPull(repo: GH.repo, number: 1301, title: "Group tabs by worktree in the iPad sidebar", state: .merged,
               review: .approved, author: GH.octocat, branch: "ipad-tabs", updated: "2d", passed: 14),
        GHPull(repo: GH.repo, number: 1297, title: "Try a second relay per region", state: .closed,
               review: nil, author: GH.octocat, branch: "relay-pair", updated: "4d", failing: 2, passed: 9),
        GHPull(repo: "sheprd-labs/herdr-site", number: 209, title: "Docs: pairing with a QR code", state: .merged,
               review: .approved, author: GH.octocat, branch: "docs-qr", updated: "6d", passed: 4),
    ]

    var body: some View {
        PRTabs {
            NavigationStack {
                list
                    .searchable(text: .constant(""), placement: .navigationBarDrawer(displayMode: .always),
                                prompt: "Title, repository, author, branch")
                    .navigationBarTitleDisplayMode(.inline)
                    .toolbar {
                        ToolbarItem(placement: .topBarLeading) {
                            Button {} label: { Image(systemName: "arrow.up.arrow.down") }
                        }
                        ToolbarItem(placement: .topBarLeading) {
                            Menu { Button("x") {} } label: {
                                Image(systemName: filtered ? "line.3.horizontal.decrease.circle.fill"
                                                           : "line.3.horizontal.decrease.circle")
                            }
                        }
                    }
            }
            .overlay(alignment: .topLeading) { if !filtered { menu } }
        }
    }

    private var list: some View {
        List {
            Section("Review requested") {
                ForEach(filtered ? Self.requestedClosed : GH.requested) { FilterRow(pull: $0, showsAuthor: true) }
            }
            Section("Mine") {
                ForEach(filtered ? Self.mineClosed : GH.mine.filter { $0.state != .merged }) {
                    FilterRow(pull: $0, showsAuthor: false)
                }
            }
        }
        .listSectionSpacing(14)
    }

    // Drawn open: a real Menu cannot be opened from a launch argument.
    private var menu: some View {
        VStack(alignment: .leading, spacing: 0) {
            item("Show Open Pull Requests", "arrow.uturn.backward", dimmed: true)
            header("State")
            check("Open", GHState.open, on: true)
            check("Draft", GHState.draft, on: true)
            check("Merged", GHState.merged, on: false)
            check("Closed", GHState.closed, on: false)
            header("Show")
            check("Review Requested", "eye", .orange, on: true)
            check("Mine", "person", .secondary, on: true)
            header("Repositories")
            submenu("sheprd-labs/sheprd")
            submenu("sheprd-labs/sheprd-apns")
            submenu("octo-org/infra")
        }
        .padding(.vertical, 6)
        .frame(width: 290)
        .background(.regularMaterial, in: .rect(cornerRadius: 22))
        .shadow(color: .black.opacity(0.25), radius: 24)
        .padding(.top, 112).padding(.leading, 18)
    }

    private func item(_ title: String, _ symbol: String, dimmed: Bool = false) -> some View {
        HStack {
            Text(title)
            Spacer()
            Image(systemName: symbol)
        }
        .foregroundStyle(dimmed ? .tertiary : .primary)
        .padding(.horizontal, 16).padding(.vertical, 11)
    }

    private func header(_ title: String) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Divider().padding(.vertical, 4)
            Text(title).font(.footnote.weight(.semibold)).foregroundStyle(.secondary)
                .padding(.horizontal, 16).padding(.top, 4).padding(.bottom, 2)
        }
    }

    private func check(_ title: String, _ state: GHState, on: Bool) -> some View {
        check(title, state.symbol, state.color, on: on)
    }

    private func check(_ title: String, _ symbol: String, _ color: Color, on: Bool) -> some View {
        HStack(spacing: 10) {
            Image(systemName: "checkmark").font(.body.weight(.semibold)).opacity(on ? 1 : 0).frame(width: 18)
            Text(title)
            Spacer()
            Image(systemName: symbol).foregroundStyle(color)
        }
        .padding(.leading, 12).padding(.trailing, 16).padding(.vertical, 10)
    }

    private func submenu(_ title: String) -> some View {
        HStack(spacing: 10) {
            Image(systemName: "checkmark").opacity(0).frame(width: 18)
            Text(title).lineLimit(1)
            Spacer()
            Image(systemName: "chevron.right").font(.footnote.weight(.semibold)).foregroundStyle(.secondary)
        }
        .padding(.leading, 12).padding(.trailing, 16).padding(.vertical, 10)
    }
}
