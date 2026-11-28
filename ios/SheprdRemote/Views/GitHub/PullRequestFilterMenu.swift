import SheprdGitHub
import SwiftUI

/// Narrows the Pull Requests tab, from beside its sort menu as Home's
/// filter is: to pull requests in some states, one section, and one
/// owner's repositories or one. Filled while it lists other than the open
/// pull requests of both sections on every repository.
struct PullRequestFilterMenu: View {
    @Binding var filter: PullRequestFilter
    /// The repositories the lists read hold, by owner.
    var owners: [(owner: String, repositories: [String])]

    var body: some View {
        let filtered = !filter.isStandard
        Menu {
            Button("Show Open Pull Requests", systemImage: "arrow.uturn.backward") { change { $0 = PullRequestFilter() } }
                .disabled(!filtered)
            Section("State") {
                ForEach(PullRequestFilter.State.allCases) { state in
                    let look = PullRequestLook(state)
                    let on = filter.states.contains(state)
                    // The last state left stays: none would list nothing.
                    toggle(look.label, systemImage: look.symbol, on: on) { value in
                        change { if value { $0.states.insert(state) } else { $0.states.remove(state) } }
                    }
                    .disabled(on && filter.states.count == 1)
                }
            }
            Section("Show") {
                toggle("Review Requested", systemImage: "eye", on: filter.showsReviewRequested) { value in
                    change { $0.showsReviewRequested = value }
                }
                .disabled(filter.showsReviewRequested && !filter.showsMine)
                toggle("Mine", systemImage: "person", on: filter.showsMine) { value in change { $0.showsMine = value } }
                    .disabled(filter.showsMine && !filter.showsReviewRequested)
            }
            repositories
        } label: {
            Image(systemName: filtered ? "line.3.horizontal.decrease.circle.fill" : "line.3.horizontal.decrease.circle")
        }
        .accessibilityLabel(filtered ? "Filter pull requests, filtered" : "Filter pull requests")
    }

    /// An owner's repositories under it, with all of them first. One chosen
    /// that the lists no longer hold stays offered, so it can be cleared.
    @ViewBuilder private var repositories: some View {
        let names = Set(owners.flatMap { [$0.owner] + $0.repositories })
        if owners.count > 1 || owners.first.map({ $0.repositories.count > 1 }) == true || filter.repository != nil {
            Section("Repositories") {
                if let chosen = filter.repository, !names.contains(chosen) {
                    repository(chosen, title: chosen)
                }
                ForEach(owners, id: \.owner) { owner in
                    let chosen = filter.repository.map { $0 == owner.owner || owner.repositories.contains($0) } ?? false
                    Menu {
                        repository(owner.owner, title: "All of \(owner.owner)")
                        Section {
                            ForEach(owner.repositories, id: \.self) { name in
                                repository(name, title: String(name.dropFirst(owner.owner.count + 1)))
                            }
                        }
                    } label: {
                        Label(owner.owner, systemImage: chosen ? "checkmark" : "folder")
                    }
                }
            }
        }
    }

    private func repository(_ name: String, title: String) -> some View {
        toggle(title, systemImage: nil, on: filter.repository == name) { value in change { $0.repository = value ? name : nil } }
    }

    /// States and sections are ticked several at a time, so the menu stays
    /// open for them; a repository, chosen alone, closes it.
    private func toggle(_ title: String, systemImage: String?, on: Bool, set: @escaping (Bool) -> Void) -> some View {
        Toggle(isOn: Binding(get: { on }, set: set)) {
            if let systemImage { Label(title, systemImage: systemImage) } else { Text(verbatim: title) }
        }
        .menuActionDismissBehavior(systemImage == nil ? .automatic : .disabled)
    }

    private func change(_ edit: (inout PullRequestFilter) -> Void) {
        withAnimation(.snappy) { edit(&filter) }
    }
}

extension PullRequestLook {
    init(_ state: PullRequestFilter.State) {
        self = switch state {
        case .open: .open
        case .draft: .draft
        case .merged: .merged
        case .closed: .closed
        }
    }
}
