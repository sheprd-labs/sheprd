import SheprdRemoteCore
import SwiftUI

/// What the app is creating, shown as a sheet over every tab.
enum Creating: Identifiable, Hashable {
    case project(device: String)
    /// A workspace, on `device` when the person came from that computer,
    /// as a new worktree of the checkout in `folder` when they came from
    /// its repository, in that repository's Herdr `session` when its
    /// computer names one.
    case workspace(device: String?, folder: String? = nil, session: String? = nil)
    /// A chat in an existing workspace, with `agent` chosen when the
    /// person picked one before asking for the form.
    case chat(device: String, workspace: String, label: String, agent: String? = nil)

    var id: String {
        switch self {
        case let .project(device): "project/\(device)"
        case let .workspace(device, folder, session): "workspace/\(device ?? "")/\(folder ?? "")/\(session ?? "")"
        case let .chat(device, workspace, _, _): "chat/\(device)/\(workspace)"
        }
    }
}

/// A chat with another agent in a workspace that already exists.
struct NewChatSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let device: String
    let workspace: String
    let label: String
    @State private var agents: [AgentChoice] = []
    @State private var agent: String
    @State private var prompt = ""
    @State private var problem: String?
    @State private var creating = false

    init(device: String, workspace: String, label: String, agent: String? = nil) {
        self.device = device
        self.workspace = workspace
        self.label = label
        _agent = State(initialValue: agent ?? "claude")
    }

    var body: some View {
        NavigationStack {
            Form {
                Section("Agent") {
                    AgentGrid(agents: agents, selection: $agent)
                    if let name = agents.first(where: { $0.id == agent })?.name {
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
            }
            .followsFirstPrompt(prompt)
            .navigationTitle("New Chat in \(label)")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    if creating { ProgressView() } else { Button("Start") { start() }.disabled(!agents.contains { $0.id == agent }) }
                }
            }
            .task {
                do {
                    let loaded = try await model.fleet.call(device) { client, token in try await client.newOptions(bearer: token) }
                    guard !Task.isCancelled else { return }
                    agents = loaded.agents
                    agent = loaded.agent(preferring: agent)?.id ?? ""
                } catch {
                    problem = error.localizedDescription
                }
            }
        }
        .interactiveDismissDisabled(creating)
    }

    private func start() {
        guard let selected = agents.first(where: { $0.id == agent }) else { return }
        let title = selected.name
        let request = NewChatRequest(
            agent: agent,
            prompt: prompt.trimmingCharacters(in: .whitespacesAndNewlines),
            start: model.startChoice(for: title).start
        )
        let (device, workspace) = (device, workspace)
        creating = true
        Task {
            defer { creating = false }
            do {
                let created = try await model.fleet.call(device) { client, token in
                    try await client.startChat(request, workspace: workspace, bearer: token)
                }
                model.recentAgent = request.agent
                dismiss()
                model.opened(created, prompt: request.prompt, start: request.start, device: device, title: title)
            } catch {
                problem = error.localizedDescription
            }
        }
    }
}

/// The agents as a grid of their marks.
struct AgentGrid: View {
    let agents: [AgentChoice]
    @Binding var selection: String

    var body: some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 92), spacing: 10)], spacing: 10) {
            ForEach(agents) { agent in
                let selected = agent.id == selection
                Button {
                    selection = agent.id
                } label: {
                    VStack(spacing: 6) {
                        AgentIcon(agent: agent.id, size: 32)
                        Text(agent.name).font(.caption.weight(.medium)).lineLimit(1)
                    }
                    .frame(maxWidth: .infinity).padding(.vertical, 10)
                    .background(selected ? AnyShapeStyle(.tint.opacity(0.16)) : AnyShapeStyle(.fill.quinary), in: .rect(cornerRadius: 14))
                    .overlay {
                        RoundedRectangle(cornerRadius: 14).strokeBorder(selected ? AnyShapeStyle(.tint) : AnyShapeStyle(.clear), lineWidth: 1.5)
                    }
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(selected ? .isSelected : [])
            }
        }
        .padding(.vertical, 4)
        .haptic(.selection, trigger: selection)
    }
}

/// The folders a workspace may open in, searchable: recent ones, then
/// each project root's.
struct FolderPicker: View {
    @Environment(\.dismiss) private var dismiss
    let options: NewOptions?
    @Binding var selection: String?
    @State private var query = ""

    var body: some View {
        List {
            let recent = filter(options?.recent ?? [])
            if !recent.isEmpty {
                Section("Recent") { ForEach(recent, id: \.self, content: row) }
            }
            ForEach(options?.roots ?? []) { root in
                let folders = filter(root.folders)
                if !folders.isEmpty {
                    Section(root.path) { ForEach(folders, id: \.self, content: row) }
                }
            }
        }
        .searchable(text: $query, prompt: "Folders")
        .navigationTitle("Project")
        .navigationBarTitleDisplayMode(.inline)
    }

    private func filter(_ folders: [String]) -> [String] {
        query.isEmpty ? folders : folders.filter { $0.localizedCaseInsensitiveContains(query) }
    }

    private func row(_ folder: String) -> some View {
        Button {
            selection = folder
            dismiss()
        } label: {
            HStack {
                Label(URL(filePath: folder).lastPathComponent, systemImage: "folder")
                Spacer()
                if folder == selection { Image(systemName: "checkmark").foregroundStyle(.tint) }
            }
        }
        .foregroundStyle(.primary)
    }
}

extension AppModel {
    /// Opens a chat just created. A first prompt that did not go, because
    /// the agent was slow to start or refused it, waits in the chat's
    /// composer, and the person is told rather than finding it gone.
    func opened(_ created: Created, prompt: String, start: ChatStart?, device: String, title: String) {
        let unsent = !prompt.isEmpty && !created.prompted
        if let pane = created.pane {
            if unsent { fleet.save(draft: Cache.Draft(text: prompt, pictures: []), for: PaneKey(device: device, pane: pane)) }
            pendingRoute = .chat(PaneKey(device: device, pane: pane), title: title)
        }
        let unset = startProblem(created, start: start, device: device)
        guard unsent else {
            if let unset { notice = unset }
            return
        }
        let unsentNotice = created.pane == nil
            ? "The chat started, but your first prompt was not sent. Open the chat from Home and send it again:\n\n\(prompt)"
            : "The chat started, but your first prompt was not sent. It is in the chat's message box: send it when the agent is ready."
        notice = unset.map { "\($0)\n\n\(unsentNotice)" } ?? unsentNotice
    }

    /// What to say about a chat that did not start as asked, if anything. A
    /// computer too old for `start` never tried, and that is what is said,
    /// with its own alert title.
    func startProblem(_ created: Created, start: ChatStart?, device: String) -> String? {
        guard start != nil else { return nil }
        switch created.started {
        case true: return nil
        case false: return Self.startMissed
        case nil:
            noticeTitle = "Update Sheprd"
            let name = computers.first { $0.device == device }.map(displayName) ?? "This computer"
            let prompt = created.prompted ? " Your first prompt ran in that mode too." : ""
            return "\(name) runs an older Sheprd that cannot start a chat in the mode you picked, so this chat is in the agent's default.\(prompt) Update it from the computer's page in Settings."
        }
    }

    /// Said when a chat may be in another mode than the one the person saw.
    static let startMissed = "The chat may not have the mode or auto-accept you picked. Check them in its Session sheet."
}

/// The first prompt of a new workspace or chat: the form's last row, which
/// grows with what is typed.
struct FirstPromptField: View {
    /// The row the form scrolls to as the field grows.
    static let id = "first-prompt"

    @Binding var prompt: String

    var body: some View {
        TextField("What should it do first? (optional)", text: $prompt, axis: .vertical)
            .lineLimit(2...6)
            .id(Self.id)
    }
}

extension View {
    /// Keeps the first prompt's row in view as it grows: a form does not
    /// scroll a growing field out from under the keyboard by itself, so its
    /// new lines were typed out of sight.
    func followsFirstPrompt(_ prompt: String) -> some View {
        ScrollViewReader { proxy in
            onChange(of: prompt) {
                proxy.scrollTo(FirstPromptField.id, anchor: .bottom)
            }
        }
    }
}
