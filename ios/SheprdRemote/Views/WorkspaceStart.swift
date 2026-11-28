import SheprdGitHub
import SheprdRemoteCore

/// What a new workspace starts from, as the desktop's new worktree dialog
/// offers it in its tabs.
enum WorkspaceStart {
    enum Kind: String, CaseIterable, Hashable {
        /// The folder itself, no worktree.
        case folder
        case newBranch
        /// A local branch no checkout has, as it is.
        case branch
        case pull
        /// A new branch named for an issue.
        case issue
        /// A checkout of the repository no workspace has open.
        case checkout

        var title: String {
            switch self {
            case .folder: "This folder"
            case .newBranch: "New branch"
            case .branch: "Branch"
            case .pull: "Pull request"
            case .issue: "Issue"
            case .checkout: "Existing worktree"
            }
        }

        var symbol: String {
            switch self {
            case .folder: "folder"
            case .newBranch: "plus"
            case .branch: "arrow.triangle.branch"
            case .pull: "arrow.triangle.pull"
            case .issue: "smallcircle.filled.circle"
            case .checkout: "square.stack.3d.up"
            }
        }
    }

    /// What the form says under the project about `kind`; `older` when the
    /// computer is too old to list sources.
    static func note(for kind: Kind, older: Bool) -> String? {
        if older {
            return "Update Sheprd on this computer to start from a branch, pull request, issue, or existing worktree."
        }
        return switch kind {
        case .folder, .newBranch: nil
        case .branch: "The worktree uses the local branch as it is."
        case .pull: "The computer fetches the pull request's branch first."
        case .issue: "A new branch named for the issue, from the checkout's branch."
        case .checkout: "Opening adds the worktree to Sheprd; nothing is created."
        }
    }

    /// What was picked in the lists, kept apart per kind so switching back
    /// and forth keeps each.
    struct Picks: Equatable {
        var branch: String?
        /// A pull request or an issue.
        var item: RepositoryItem?
        var checkout: NewSources.Checkout?

        /// The picked pull request or issue, when it is one `kind` takes.
        func item(for kind: Kind) -> RepositoryItem? {
            switch (kind, item?.kind) {
            case (.pull, .pull), (.issue, .issue): item
            default: nil
            }
        }

        /// Whether `kind` has what it needs. A new branch is typed, and
        /// checked by the form.
        func chosen(for kind: Kind) -> Bool {
            switch kind {
            case .folder, .newBranch: true
            case .branch: branch != nil
            case .pull, .issue: item(for: kind) != nil
            case .checkout: checkout != nil
            }
        }

        /// The name the workspace gets unless the person types one: a pull
        /// request's or issue's, as the desktop names its checkout, cut to
        /// the longest name a workspace may have.
        func title(for kind: Kind) -> String? {
            item(for: kind).map { Self.bounded($0.label) }
        }

        /// The most characters a workspace name may have, as the daemon
        /// counts them (`MAX_LABEL_CHARS`, in Unicode scalars).
        static let maxTitle = 200

        static func bounded(_ title: String) -> String {
            guard title.unicodeScalars.count > maxTitle else { return title }
            let cut = String(String.UnicodeScalarView(title.unicodeScalars.prefix(maxTitle)))
            return cut.trimmingCharacters(in: .whitespaces)
        }

        /// The request's branch, source, and default title for `kind`, with
        /// `typed` as the new branch.
        func request(for kind: Kind, branch typed: String) -> (branch: String, source: NewWorkspaceRequest.Source?, title: String?) {
            switch kind {
            case .folder:
                return ("", nil, nil)
            case .newBranch:
                return (typed.trimmingCharacters(in: .whitespaces), nil, nil)
            case .branch:
                return (branch ?? "", nil, nil)
            case .pull:
                guard let item = item(for: kind), let head = item.head else { return ("", nil, nil) }
                return ("", .pull(number: item.number, head: head, headOwner: item.headOwner), title(for: kind))
            case .issue:
                guard let item = item(for: kind) else { return ("", nil, nil) }
                return (item.issueBranch, nil, title(for: kind))
            case .checkout:
                return ("", checkout.map { .checkout(path: $0.path) }, nil)
            }
        }
    }
}
