import SheprdGitHub
import SheprdRemoteCore
import SwiftUI

/// Creates the checkout on the chosen computer, then opens its first workspace.
struct NewProjectSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let device: String
    @State private var options: NewOptions?
    @State private var root = ""
    @State private var name = ""
    @State private var cloning = false
    @State private var source = ""
    @State private var agent = ""
    @State private var prompt = ""
    @State private var working = false
    @State private var problem: String?
    @State private var showsCreationError = false
    /// If opening the workspace failed, retry only that step.
    @State private var createdPath: String?

    private var request: NewProjectRequest? {
        NewProjectRequest(root: root, name: name, clone: cloning ? source : nil)
    }

    var body: some View {
        NavigationStack {
            Form {
                if let createdPath {
                    LabeledContent("Project created", value: createdPath)
                } else {
                    Section {
                        Picker("Start from", selection: $cloning) {
                            Text("Empty").tag(false)
                            Text("Clone").tag(true)
                        }
                        .pickerStyle(.segmented)
                        if cloning {
                            TextField("Repository URL or owner/name", text: $source)
                                .textInputAutocapitalization(.never).autocorrectionDisabled()
                            NavigationLink("Browse GitHub repositories") {
                                ProjectRepositories { repo in
                                    source = repo.fullName
                                    if name.isEmpty { name = repo.name }
                                }
                            }
                        }
                        TextField("Project name", text: $name)
                            .textInputAutocapitalization(.never).autocorrectionDisabled()
                        if let options {
                            if options.roots.isEmpty {
                                Text("Open an existing project on this computer to offer its parent folder here.")
                            } else {
                                Picker("Location", selection: $root) {
                                    ForEach(options.roots) { Text($0.path).tag($0.path) }
                                }
                            }
                        } else if problem == nil {
                            ProgressView("Loading project locations…")
                        }
                    } header: {
                        if let computer = model.fleet.machines.first(where: { $0.id == device })?.computer {
                            Text("Project on \(model.displayName(computer))")
                        }
                    } footer: {
                        if cloning {
                            Text("Cloning uses the selected computer’s Git credentials. For a private repository, make sure that computer has access.")
                        }
                    }
                }
                if let options {
                    Section("Open workspace with") {
                        if options.agents.isEmpty {
                            Text("No agents are available on this computer.")
                        } else {
                            AgentGrid(agents: options.agents, selection: $agent)
                        }
                        if let choice = options.agents.first(where: { $0.id == agent }) {
                            StartChoiceRow(agent: choice.name)
                        }
                    }
                    Section("First prompt") { FirstPromptField(prompt: $prompt) }
                }
                if options == nil, let problem {
                    Section {
                        Text(problem).foregroundStyle(.red)
                        Button("Retry") { Task { await load() } }
                    }
                }
            }
            .followsFirstPrompt(prompt)
            .disabled(working)
            .navigationTitle("New Project")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() }.disabled(working) }
                ToolbarItem(placement: .confirmationAction) {
                    if working {
                        ProgressView()
                    } else {
                        Button(createdPath == nil ? "Create" : "Open") { create() }
                            .disabled((createdPath == nil && request == nil) || agent.isEmpty)
                    }
                }
            }
            .task { if options == nil { await load() } }
            .alert(createdPath == nil ? "Couldn’t create project" : "Couldn’t open workspace",
                   isPresented: $showsCreationError) {
                Button("OK", role: .cancel) {}
            } message: {
                Text(problem ?? "")
            }
        }
        .interactiveDismissDisabled(working)
    }

    private func load() async {
        problem = nil
        do {
            let loaded = try await model.fleet.call(device) { client, token in try await client.newOptions(bearer: token) }
            root = loaded.roots.first?.path ?? ""
            agent = loaded.agent(preferring: model.recentAgent ?? "claude")?.id ?? ""
            options = loaded
        } catch where !Task.isCancelled { problem = error.localizedDescription } catch {}
    }

    private func create() {
        guard !working, let choice = options?.agents.first(where: { $0.id == agent }) else { return }
        working = true
        problem = nil
        Task {
            defer { working = false }
            do {
                let path: String
                if let createdPath {
                    path = createdPath
                } else if let request {
                    path = try await model.fleet.call(device) { client, token in try await client.createProject(request, bearer: token) }
                    createdPath = path
                } else { return }
                let workspace = NewWorkspaceRequest(cwd: path, agent: choice.id,
                                                    prompt: prompt.trimmingCharacters(in: .whitespacesAndNewlines),
                                                    start: model.startChoice(for: choice.name).start)
                let created = try await model.fleet.call(device) { client, token in try await client.createWorkspace(workspace, bearer: token) }
                model.recentAgent = choice.id
                dismiss()
                model.opened(created, prompt: workspace.prompt, start: workspace.start, device: device, title: choice.name)
            } catch {
                problem = error.localizedDescription
                showsCreationError = true
            }
        }
    }
}

private struct ProjectRepositories: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let select: (GitHubRepository) -> Void
    @State private var read: RepositoriesRead?
    @State private var query = ""

    var body: some View {
        List {
            switch read {
            case nil: ProgressView("Loading repositories…")
            case .noAccount: Text("Sign in to GitHub or enter a repository URL in the project form.")
            case .failed: Button("Retry loading repositories") { Task { await load() } }
            case let .found(repos):
                let matches = repos.filter { query.isEmpty || $0.fullName.localizedCaseInsensitiveContains(query) }
                if matches.isEmpty { Text("No repositories found") }
                ForEach(matches) { repo in
                    Button {
                        select(repo)
                        dismiss()
                    } label: {
                        VStack(alignment: .leading) {
                            Text(verbatim: repo.fullName)
                            if repo.isPrivate {
                                Text("Private").font(.caption).foregroundStyle(.secondary)
                            }
                        }
                    }
                }
            }
        }
        .navigationTitle("Repositories")
        .searchable(text: $query)
        .task { await load() }
    }

    private func load() async {
        read = nil
        read = await GitHubReads.reader.repositories(source: model.fleet.gitHubLenders)
    }
}
