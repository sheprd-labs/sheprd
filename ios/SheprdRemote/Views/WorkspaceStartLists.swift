import SheprdGitHub
import SheprdRemoteCore
import SwiftUI

/// The local branches no checkout has, searchable.
struct BranchList: View {
    @Environment(\.dismiss) private var dismiss
    let branches: [String]
    @Binding var selection: String?
    @State private var query = ""

    var body: some View {
        let shown = query.isEmpty ? branches : branches.filter { $0.localizedCaseInsensitiveContains(query) }
        List(shown, id: \.self) { branch in
            PickRow(selected: branch == selection) {
                Label(branch, systemImage: "arrow.triangle.branch").font(.body.monospaced()).lineLimit(1)
            } pick: {
                selection = branch
                dismiss()
            }
        }
        .overlay {
            if shown.isEmpty {
                ContentUnavailableView(
                    branches.isEmpty ? "Every branch has a worktree" : "No matching branches",
                    systemImage: "arrow.triangle.branch"
                )
            }
        }
        .searchable(text: $query, prompt: "Branches")
        .navigationTitle("Branch")
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// The checkouts of the repository no workspace has open.
struct CheckoutList: View {
    @Environment(\.dismiss) private var dismiss
    let checkouts: [NewSources.Checkout]
    @Binding var selection: NewSources.Checkout?
    @State private var query = ""

    var body: some View {
        let shown = query.isEmpty ? checkouts : checkouts.filter {
            $0.path.localizedCaseInsensitiveContains(query) || ($0.branch ?? "").localizedCaseInsensitiveContains(query)
        }
        List(shown) { checkout in
            PickRow(selected: checkout == selection) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(checkout.branch ?? "detached").font(.body.monospaced()).lineLimit(1)
                    Text(checkout.path).font(.caption.monospaced()).foregroundStyle(.secondary).lineLimit(1).truncationMode(.head)
                }
            } pick: {
                selection = checkout
                dismiss()
            }
        }
        .overlay {
            if shown.isEmpty {
                ContentUnavailableView(
                    checkouts.isEmpty ? "Every worktree is open" : "No matching worktrees",
                    systemImage: "square.stack.3d.up"
                )
            }
        }
        .searchable(text: $query, prompt: "Worktrees")
        .navigationTitle("Existing worktree")
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// The repository's open pull requests or issues, read from GitHub with
/// the phone's own sign-in or a token a computer lends.
struct RepositoryItemList: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let kind: RepositoryItem.Kind
    let repository: NewSources.Repository
    @Binding var selection: RepositoryItem?
    @State private var read: RepositoryItemsRead?
    @State private var query = ""

    var body: some View {
        let items: [RepositoryItem] = switch read {
        case let .found(found): (kind == .pull ? found.pulls : found.issues).filter { $0.matches(query) }
        default: []
        }
        List(items) { item in
            PickRow(selected: item == selection) {
                ItemRow(item: item)
            } pick: {
                selection = item
                dismiss()
            }
        }
        .overlay { overlay(empty: items.isEmpty) }
        .searchable(text: $query, prompt: kind == .pull ? "Number, title, author, branch" : "Number, title, author")
        .navigationTitle(kind == .pull ? "Pull request" : "Issue")
        .navigationBarTitleDisplayMode(.inline)
        .task { await load() }
        .refreshable { await load() }
    }

    @ViewBuilder
    private func overlay(empty: Bool) -> some View {
        switch read {
        case nil:
            ProgressView()
        case .noAccount:
            ContentUnavailableView(
                "Sign in to GitHub",
                systemImage: "person.crop.circle.badge.questionmark",
                description: Text("Sign in under Settings > GitHub, or let a computer share its account, to list \(kind == .pull ? "pull requests" : "issues").")
            )
        case .failed:
            ContentUnavailableView(
                "GitHub could not be read",
                systemImage: "exclamationmark.triangle",
                description: Text("\(repository.owner)/\(repository.repo) on \(repository.host). Pull to try again.")
            )
        case .found where empty:
            ContentUnavailableView(kind == .pull ? "No open pull requests" : "No open issues", systemImage: kind == .pull ? "arrow.triangle.pull" : "smallcircle.filled.circle")
        case .found:
            EmptyView()
        }
    }

    private func load() async {
        let found = await GitHubReads.reader.repositoryItems(
            host: repository.host, owner: repository.owner, repo: repository.repo, source: model.fleet.gitHubLenders
        )
        guard !Task.isCancelled else { return }
        read = found
    }
}

/// One pull request or issue: its number and title, then who opened it
/// and, for a pull request, its branch.
private struct ItemRow: View {
    let item: RepositoryItem

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text("#\(item.number)").font(.subheadline.monospacedDigit()).foregroundStyle(.secondary)
                Text(item.title).lineLimit(2)
            }
            HStack(spacing: 6) {
                if !item.author.isEmpty { Text(item.author) }
                if let head = item.head { Text(head).font(.caption.monospaced()).lineLimit(1).truncationMode(.middle) }
                if item.isDraft { Text("draft").padding(.horizontal, 5).background(.fill.tertiary, in: .capsule) }
            }
            .font(.caption)
            .foregroundStyle(.secondary)
        }
    }
}

/// A list row that picks its value, checked when it is the one picked.
private struct PickRow<Content: View>: View {
    let selected: Bool
    @ViewBuilder let content: Content
    let pick: () -> Void

    var body: some View {
        Button(action: pick) {
            HStack {
                content
                Spacer()
                if selected { Image(systemName: "checkmark").foregroundStyle(.tint) }
            }
            .contentShape(.rect)
        }
        .foregroundStyle(.primary)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}
