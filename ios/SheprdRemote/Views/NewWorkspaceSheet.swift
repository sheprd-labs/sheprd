import SheprdGitHub
import SheprdRemoteCore
import SwiftUI

/// New workspace A: one form for the computer, the project, what its
/// worktree starts from (as the desktop's new worktree dialog: a new
/// branch, a local branch, a pull request, an issue, or a checkout that
/// exists), what it opens in, the agent, and an optional first prompt.
struct NewWorkspaceSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var device: String?
    /// The computer was chosen for its load. It stays the one chosen, as
    /// the desktop's ranking settles, so the form does not move under the
    /// person with every sample.
    @State private var automatic = false
    @State private var options: NewOptions?
    /// The computer `options` came from. The form reappears after the
    /// folder picker, which restarts `load`; this keeps the chosen folder.
    @State private var loadedFrom: String?
    @State private var folder: String?
    @State private var start: WorkspaceStart.Kind
    /// What the folder's repository may start from; `nil` until read, or
    /// from a computer too old to say.
    @State private var sources: NewSources?
    /// Whether the computer answered for `sources` at all: an older one
    /// offers only a new branch and a chat.
    @State private var sourcesKnown = false
    /// The computer and folder `sources` came from. The form reappears
    /// after each list it pushes, which restarts the read; this keeps the
    /// picks made there.
    @State private var sourcesFrom: String?
    @State private var picks = WorkspaceStart.Picks()
    /// Proposed up front, as on the desktop, so Create needs no typing.
    @State private var branch = NewWorkspaceRequest.proposedBranch()
    /// The workspace's name; empty, the computer names it.
    @State private var title = ""
    @State private var opens = NewWorkspaceRequest.Opens.chat
    @State private var agent = "claude"
    @State private var prompt = ""
    @State private var problem: String?
    @State private var creating = false

    /// The checkout the person came from, chosen once its computer's
    /// folders load, unless they pick another computer.
    private let preset: (device: String, folder: String)?
    /// The checkout's Herdr session, which the workspace opens in while
    /// the checkout stays the one chosen.
    private let session: String?

    init(device: String?, folder: String? = nil, session: String? = nil) {
        _device = State(initialValue: device)
        _start = State(initialValue: folder != nil ? .newBranch : .folder)
        preset = device.flatMap { device in folder.map { (device, $0) } }
        self.session = session
    }

    var body: some View {
        NavigationStack {
            Form {
                computerSection
                projectSection
                if sourcesKnown {
                    Section("Opens in") {
                        Picker("Opens in", selection: $opens.animation()) {
                            Text("Chat").tag(NewWorkspaceRequest.Opens.chat)
                            Text("Terminal").tag(NewWorkspaceRequest.Opens.shell)
                        }
                        .pickerStyle(.segmented)
                        .labelsHidden()
                    }
                }
                if opens == .chat {
                    Section("Agent") {
                        AgentGrid(agents: options?.agents ?? [], selection: $agent)
                        if let name = options?.agents.first(where: { $0.id == agent })?.name {
                            StartChoiceRow(agent: name)
                        }
                    }
                    Section {
                        FirstPromptField(prompt: $prompt)
                    } header: {
                        Text("First prompt")
                    } footer: {
                        if let problem { Text(problem).foregroundStyle(.red) }
                    }
                } else if let problem {
                    Section { Text(problem).foregroundStyle(.red) }
                }
            }
            .followsFirstPrompt(prompt)
            .navigationTitle("New Workspace")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    if creating {
                        ProgressView()
                    } else {
                        Button("Create") { create() }.disabled(!ready)
                    }
                }
            }
            .onAppear {
                guard device == nil else { return }
                let online = model.fleet.machines.filter { if case .online = $0.status { true } else { false } }
                device = online.first { $0.id == model.recentDevice }?.id ?? online.first?.id
            }
            .task(id: device) { await load() }
            .task(id: sourcesKey) { await loadSources() }
        }
        .interactiveDismissDisabled(creating)
    }

    private var computerSection: some View {
        Section {
            Picker("Computer", selection: computerChoice) {
                if offersLeastBusy {
                    Text(leastBusyName).tag(String?.none)
                }
                ForEach(model.fleet.machines) { machine in
                    Text(model.displayName(machine.computer)).tag(Optional(machine.id))
                }
            }
            .pickerStyle(.menu)
        } header: {
            Text("Computer")
        } footer: {
            if automatic, let load = model.fleet.machines.first(where: { $0.id == device })?.home?.load {
                Text("Chosen for its room: \(load.spareCores, format: .number.precision(.fractionLength(0...1))) cores spare, \(load.memory)% memory in use.")
            }
        }
    }

    private var projectSection: some View {
        Section {
            NavigationLink {
                FolderPicker(options: options, selection: $folder)
            } label: {
                if let folder {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(URL(filePath: folder).lastPathComponent).font(.headline)
                        Text(folder).font(.caption.monospaced()).foregroundStyle(.secondary).lineLimit(1).truncationMode(.head)
                    }
                } else if options == nil, problem == nil {
                    HStack { Text("Loading folders…").foregroundStyle(.secondary); Spacer(); ProgressView() }
                } else {
                    Text("Choose a folder").foregroundStyle(.secondary)
                }
            }
            .disabled(options == nil)
            Picker("Start from", selection: $start.animation()) {
                ForEach(offered, id: \.self) { kind in
                    Label(kind.title, systemImage: kind.symbol).tag(kind)
                }
            }
            .pickerStyle(.menu)
            startRow
            TextField(picks.title(for: start) ?? "Title (optional)", text: $title)
        } header: {
            Text("Project")
        } footer: {
            if let note = WorkspaceStart.note(for: start, older: !sourcesKnown && folder != nil) {
                Text(note)
            }
        }
    }

    /// The row choosing what `start` names.
    @ViewBuilder
    private var startRow: some View {
        switch start {
        case .folder:
            EmptyView()
        case .newBranch:
            TextField("Branch", text: $branch)
                .textInputAutocapitalization(.never).autocorrectionDisabled()
                .font(.body.monospaced())
        case .branch:
            NavigationLink {
                BranchList(branches: sources?.branches ?? [], selection: $picks.branch)
            } label: {
                PickedRow(text: picks.branch, placeholder: "Choose a branch", monospaced: true)
            }
        case .pull, .issue:
            NavigationLink {
                if let repository = sources?.repository {
                    RepositoryItemList(kind: start == .pull ? .pull : .issue, repository: repository, selection: $picks.item)
                }
            } label: {
                PickedRow(text: picks.item(for: start)?.label, placeholder: start == .pull ? "Choose a pull request" : "Choose an issue")
            }
        case .checkout:
            NavigationLink {
                CheckoutList(checkouts: sources?.checkouts ?? [], selection: $picks.checkout)
            } label: {
                PickedRow(text: picks.checkout.map { $0.branch ?? URL(filePath: $0.path).lastPathComponent }, placeholder: "Choose a worktree", monospaced: true)
            }
        }
    }

    /// What the form offers to start from: everything once the computer
    /// says what the repository has, only the folder or a new branch
    /// before, and pull requests and issues only for a repository a forge
    /// hosts.
    private var offered: [WorkspaceStart.Kind] {
        WorkspaceStart.Kind.allCases.filter { kind in
            switch kind {
            case .folder, .newBranch: true
            case .branch, .checkout: sources != nil
            case .pull, .issue: sources?.repository != nil
            }
        }
    }

    /// The online computers that can take a workspace, best first.
    private var ranked: [Home.Candidate] {
        let candidates = model.fleet.machines.compactMap { machine -> Home.Candidate? in
            guard case .online = machine.status, let home = machine.home else { return nil }
            return Home.Candidate(id: machine.id, label: model.displayName(machine.computer), home: home)
        }
        return Home.leastBusy(candidates, current: device, repository: repository)
    }

    /// The repository of the chosen folder, by the name every computer
    /// knows it by, else the folder's own name.
    private var repository: String? {
        guard let folder else { return nil }
        let homes = model.fleet.machines.lazy.compactMap(\.home)
        return homes.compactMap { $0.workspaces.first { $0.cwd == folder }?.worktree?.label }.first
            ?? URL(filePath: folder).lastPathComponent
    }

    /// One computer alone is no choice.
    private var offersLeastBusy: Bool { ranked.count > 1 }

    private var leastBusyName: String {
        guard automatic, let machine = model.fleet.machines.first(where: { $0.id == device }) else { return "Least busy" }
        return "Least busy: \(model.displayName(machine.computer))"
    }

    /// The picker's selection: a computer, or nil for the least busy one.
    private var computerChoice: Binding<String?> {
        Binding {
            automatic && offersLeastBusy ? nil : device
        } set: { choice in
            automatic = choice == nil
            if let chosen = choice ?? ranked.first?.id { device = chosen }
        }
    }

    private var ready: Bool {
        // Until this computer says what it can do, a choice made for the
        // last one (a terminal, say) could reach one too old to honour it.
        guard folder != nil, sourcesFrom == sourcesKey, opens == .shell || !agent.isEmpty else { return false }
        if start == .newBranch { return Self.plausibleBranch(branch) }
        return picks.chosen(for: start)
    }

    /// The daemon checks the branch properly; this only catches typos
    /// before the round trip.
    static func plausibleBranch(_ name: String) -> Bool {
        let name = name.trimmingCharacters(in: .whitespaces)
        return !name.isEmpty && !name.contains(" ") && !name.contains("..") && !name.hasPrefix("-")
    }

    /// Sources are read again for each computer and folder.
    private var sourcesKey: String { "\(device ?? "")\n\(folder ?? "")" }

    private func load() async {
        // Choosing the computer here would change the task's id and
        // cancel this load; `onAppear` chooses it first.
        guard let device, device != loadedFrom else { return }
        // Moving to another computer keeps the project when it has it;
        // paths differ between computers.
        let project = repository
        let name = folder.map { URL(filePath: $0).lastPathComponent }
        options = nil
        folder = nil
        problem = nil
        do {
            let loaded = try await model.fleet.call(device) { client, token in try await client.newOptions(bearer: token) }
            options = loaded
            loadedFrom = device
            let home = model.fleet.machines.first { $0.id == device }?.home
            let checkout = project.flatMap { home?.checkout(ofRepository: $0) }
            folder = (preset?.device == device ? preset?.folder : nil)
                ?? checkout.flatMap { loaded.folders.contains($0) ? $0 : nil }
                ?? loaded.folders.first { URL(filePath: $0).lastPathComponent == name }
                ?? loaded.recent.first
            agent = loaded.agent(preferring: agent)?.id ?? ""
        } catch where !Task.isCancelled {
            problem = error.localizedDescription
        } catch {}
    }

    /// Reads what the chosen folder's repository may start from. Picks
    /// made for another folder go; a start it no longer offers falls back
    /// to a new branch.
    private func loadSources() async {
        guard sourcesKey != sourcesFrom else { return }
        picks = WorkspaceStart.Picks()
        sources = nil
        guard let device, let folder else { return }
        do {
            let read = try await model.fleet.call(device) { client, token in
                try await client.newSources(cwd: folder, bearer: token)
            }
            guard !Task.isCancelled else { return }
            sources = read
            sourcesKnown = read != nil
        } catch {
            // Not a repository, or not reachable: a new branch still is.
            guard !Task.isCancelled else { return }
            sourcesKnown = true
        }
        sourcesFrom = sourcesKey
        if !offered.contains(start) { start = .newBranch }
        if !sourcesKnown { opens = .chat }
    }

    private func create() {
        guard let device, let folder else { return }
        let chatTitle = opens == .shell ? "Terminal" : options?.agents.first { $0.id == agent }?.name ?? "Chat"
        let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
        let made = picks.request(for: start, branch: branch)
        let request = NewWorkspaceRequest(
            cwd: folder,
            branch: made.branch,
            agent: agent,
            prompt: opens == .chat ? prompt.trimmingCharacters(in: .whitespacesAndNewlines) : "",
            start: opens == .chat ? model.startChoice(for: chatTitle).start : nil,
            session: preset?.device == device && preset?.folder == folder ? session : nil,
            title: trimmed.isEmpty ? made.title : trimmed,
            source: made.source,
            opens: opens == .shell ? .shell : nil
        )
        creating = true
        Task {
            defer { creating = false }
            do {
                let created = try await model.fleet.call(device) { client, token in
                    try await client.createWorkspace(request, bearer: token)
                }
                dismiss()
                if request.opens == .shell {
                    if let pane = created.pane {
                        model.pendingRoute = .term(PaneKey(device: device, pane: pane), title: request.title ?? chatTitle)
                    }
                    return
                }
                model.recentAgent = request.agent
                model.opened(created, prompt: request.prompt, start: request.start, device: device, title: chatTitle)
            } catch {
                problem = error.localizedDescription
            }
        }
    }
}

/// A picked value, or what to pick, as a form row.
private struct PickedRow: View {
    let text: String?
    let placeholder: String
    var monospaced = false

    var body: some View {
        if let text {
            Text(text).font(monospaced ? .body.monospaced() : .body).lineLimit(1).truncationMode(.middle)
        } else {
            Text(placeholder).foregroundStyle(.secondary)
        }
    }
}
