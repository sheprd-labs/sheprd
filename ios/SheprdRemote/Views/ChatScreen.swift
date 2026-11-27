import SheprdRemoteCore
import SwiftUI
import UIKit

/// Chat B: the latest turn as a document (the prompt as a heading, steps
/// folded to one line, the answer, then its diffs folded to a line each),
/// the running step pinned under the header, earlier turns summed up, and
/// questions in a bottom sheet. Once the prompt scrolls off the top, its
/// first three lines stay pinned there.
struct ChatScreen: View {
    @Environment(AppModel.self) private var model
    let key: PaneKey
    let title: String
    @State private var store: ChatStore?
    @State private var showingSession = false
    @State private var showingPanel = false
    @State private var panelDetent = PresentationDetent.medium
    @State private var filter = TurnFilterState()
    @Environment(\.horizontalSizeClass) private var width
    @Environment(\.dismiss) private var dismiss
    @Environment(\.workspaceDestination) private var workspaceDestination

    private var item: ChatItem? {
        model.fleet.chatItems.first { $0.key == key }
    }

    /// The terminal tab whose agent this conversation is, when it is a
    /// terminal agent's rather than a chat's.
    private var terminal: TerminalItem? {
        guard item == nil else { return nil }
        return model.fleet.terminalItems.first { $0.computer.device == key.device && $0.agent?.pane == key.pane }
    }

    private var workspace: Home.Workspace? {
        item?.workspace ?? terminal?.workspace
    }

    /// The chat's name as the computer last sent it, so a rename shows at
    /// once; the route's title until the home arrives.
    private var name: String {
        item?.chat.title ?? terminal?.title ?? title
    }

    private var promptCount: Int {
        store?.transcript.map { ChatTurns.promptCount(in: $0.entries) } ?? 0
    }

    private var subtitle: String {
        store?.transcript.map { sessionLine(model.fleet.chat($0, for: key)) } ?? ""
    }

    var body: some View {
        Group {
            if let store, let transcript = store.transcript {
                ChatDocument(
                    key: key,
                    store: store,
                    transcript: model.fleet.chat(transcript, for: key),
                    terminal: terminal,
                    name: name,
                    subtitle: subtitle,
                    canOpenSession: terminal == nil,
                    openSession: { showingSession = true },
                    filter: $filter
                )
            } else if let problem = store?.problem {
                ContentUnavailableView {
                    Label("Can't show this conversation", systemImage: "text.bubble")
                } description: {
                    Text(problem)
                } actions: {
                    if let route = terminal?.terminalRoute {
                        RouteLink(route) { Text("Open the terminal") }.buttonStyle(.glassProminent)
                    }
                }
            } else {
                ProgressView("Loading the chat…")
            }
        }
        // The new tab screen offers the modes a chat with this agent listed.
        .onChange(of: store?.transcript.map(StartChoice.offered(in:)), initial: true) {
            guard let transcript = store?.transcript else { return }
            model.updateStart(for: transcript.agent) { $0.learn(from: transcript) }
        }
        // The title ribbon below the bar draws the chat's full name; the
        // navigation title is what Stage Manager and the app switcher use.
        .navigationTitle(LockState.shared.hidesNames ? "Sheprd" : name)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar(.hidden, for: .tabBar)
        .hidesConnectionToast()
        .toolbar {
            // The space pill takes the bar's leading side so the workspace
            // name stays visible alongside the trailing controls.
            if let workspace {
                ToolbarItem(placement: .topBarLeading) {
                    spacePill(workspace)
                }
            }
            // Keeps the system from drawing a second truncated title between
            // the space pill and the trailing buttons; the full title lives in
            // the ribbon right under the bar.
            ToolbarItem(placement: .principal) {
                EmptyView()
            }
            if let route = terminal?.terminalRoute {
                ToolbarItem(placement: .topBarTrailing) {
                    RouteLink(route) { Image(systemName: "apple.terminal") }
                        .accessibilityLabel("Open the terminal")
                }
            }
            if promptCount > 0 {
                ToolbarItem(placement: .topBarTrailing) {
                    PromptsFilterButton(active: filter.promptsOnly, count: promptCount) {
                        withAnimation(.snappy(duration: 0.34)) {
                            filter.toggleFilter()
                        }
                    }
                }
            }
            // The desktop rail's sections: sub-agents, shells, the plan,
            // changed files, and pull requests, once the agent reports any.
            if let transcript = store?.transcript, transcript.hasActivityPanel {
                ToolbarItem(placement: .topBarTrailing) {
                    AgentPanelButton(transcript: transcript) { showingPanel = true }
                }
            }
            if let usage = store?.transcript?.usage {
                ToolbarItem(placement: .topBarTrailing) {
                    Button { showingSession = true } label: { ContextRing(percent: usage.percent) }
                }
            }
        }
        .sheet(isPresented: $showingSession) {
            if let store {
                SessionSheet(key: key, store: store, name: name) {
                    showingSession = false
                    dismiss()
                }
                .presentationDetents([.medium, .large])
            }
        }
        .sheet(isPresented: $showingPanel) {
            if let store {
                AgentPanel(key: key, store: store)
                    .presentationDetents([.medium, .large], selection: $panelDetent)
                    .presentationBackground(Color(.systemGroupedBackground))
            }
        }
        .onAppear {
            #if DEBUG
            if UserDefaults.standard.bool(forKey: "promptsOnly") { filter.promptsOnly = true }
            if key == .sample {
                store = ChatStore(key: key, sample: .sample)
                if UserDefaults.standard.bool(forKey: "session") { showingSession = true }
                return
            }
            if key == .activitySample {
                store = ChatStore(key: key, sample: .activitySample)
                openPanelForLaunchArgument()
                return
            }
            if key == .effectsSample {
                let play = UserDefaults.standard.string(forKey: "samplePlay")
                let sample = ChatStore(key: key, sample: .effectsSample(screen: play.flatMap(ScreenEffect.init(rawValue:)) ?? .confetti))
                store = sample
                if let play {
                    Task {
                        try? await Task.sleep(for: .seconds(1))
                        sample.playSample(play)
                    }
                }
                return
            }
            #endif
            store = model.fleet.openChat(key)
            model.recentDevice = key.device
            #if DEBUG
            // The simulator loop's `-session YES` opens the session sheet.
            if UserDefaults.standard.bool(forKey: "session") { showingSession = true }
            // `-agentPanel YES` opens the activity panel, `-agentPanel large`
            // at full height.
            openPanelForLaunchArgument()
            #endif
        }
        .onDisappear {
            if let store { model.fleet.closeChat(store) }
        }
    }

    #if DEBUG
    private func openPanelForLaunchArgument() {
        let defaults = UserDefaults.standard
        if defaults.string(forKey: "agentPanel") == "large" { panelDetent = .large }
        if defaults.bool(forKey: "agentPanel") || defaults.string(forKey: "agentPanel") == "large" { showingPanel = true }
    }
    #endif

    @ViewBuilder
    private func spacePill(_ workspace: Home.Workspace) -> some View {
        let workspaceKey = WorkspaceKey(device: key.device, workspace: workspace.id)
        switch workspaceDestination {
        case .pending:
            Button { model.pendingWorkspace = workspaceKey } label: {
                spacePillLabel(workspace)
            }
            .accessibilityLabel("Go to \(workspace.label)")
        case .dismiss:
            Button { dismiss() } label: {
                spacePillLabel(workspace)
            }
            .accessibilityLabel("Go to \(workspace.label)")
        case .push:
            NavigationLink(value: Route.workspace(workspaceKey)) {
                spacePillLabel(workspace)
            }
            .accessibilityLabel("Go to \(workspace.label)")
        }
    }

    private func spacePillLabel(_ workspace: Home.Workspace) -> some View {
        HStack(spacing: 5) {
            Image(systemName: "square.stack.3d.up")
                .foregroundStyle(.tint)
            Text(LockState.shared.hidesNames ? "Space" : workspace.label)
                .font(.subheadline.weight(.semibold))
                .lineLimit(1)
        }
        .frame(maxWidth: width == .regular ? 240 : 150, alignment: .leading)
    }

    /// "Opus 5.5 · High · Bypass permissions", from the session's settings.
    private func sessionLine(_ transcript: Transcript) -> String {
        let model = transcript.options.first { $0.category == .model }.map(currentName)
        let effort = transcript.options.first { $0.category == .thoughtLevel }.map(currentName)
        let mode = transcript.modes.flatMap { modes in modes.available.first { $0.id == modes.current }?.name }
        return [model, effort, mode].compactMap { $0 }.joined(separator: " · ")
    }

    private func currentName(_ option: ConfigOption) -> String {
        switch option.kind {
        case let .select(choices, current): choices.first { $0.id == current }?.name ?? current
        case let .toggle(current): current ? "On" : "Off"
        }
    }
}

struct ContextRing: View {
    var percent: Int
    var size: CGFloat = 20
    var lineWidth: CGFloat = 3

    var body: some View {
        ZStack {
            Circle().stroke(.quaternary, lineWidth: lineWidth)
            Circle().trim(from: 0, to: Double(percent) / 100)
                .stroke(percent > 80 ? AnyShapeStyle(.orange) : AnyShapeStyle(.tint), style: StrokeStyle(lineWidth: lineWidth, lineCap: .round))
                .rotationEffect(.degrees(-90))
        }
        .frame(width: size, height: size)
        .accessibilityLabel("Context \(percent) percent full")
    }
}

private struct ChatDocument: View {
    @Environment(AppModel.self) private var model
    let key: PaneKey
    let store: ChatStore
    let transcript: Transcript
    /// The terminal tab whose agent this is; replies go to its terminal.
    let terminal: TerminalItem?
    let name: String
    let subtitle: String
    let canOpenSession: Bool
    let openSession: () -> Void
    @Binding var filter: TurnFilterState
    @State private var problem: String?
    @State private var dismissedQuestion: PermissionId?
    @State private var position = ScrollPosition(edge: .bottom)
    /// Scrolled away from the latest, so the jump back shows.
    @State private var away = false
    /// How many of the latest turns show; earlier ones load on request.
    @State private var shownTurns = 5
    /// The latest prompt has scrolled up past the top of the chat.
    @State private var promptAbove = false
    /// Scrolled past the end, as when the content shrank under a resting
    /// offset: the screen shows nothing until a drag.
    @State private var overshot = false
    @State private var scrolling = false
    /// Resting on the latest message, not merely near it as `away` allows:
    /// only then does the chat follow it back there.
    @State private var atLatest = true
    /// Turn to scroll to right after expanding from prompts-only mode.
    @State private var pendingJump: EntryId?
    @Environment(\.scenePhase) private var scenePhase

    /// Lines stay this wide at most on an iPad or an unfolded iPhone, so
    /// the answer reads like a page rather than a banner.
    static let readableWidth: CGFloat = 680

    var body: some View {
        let turns = ChatTurn.split(transcript.entries)
        let promptCount = ChatTurns.promptCount(in: transcript.entries)
        let visible = filter.visibleTurns(from: turns, shownTurns: shownTurns)
        let turnRows: [(id: EntryId, entries: ArraySlice<Entry>)] = visible.turns.compactMap { turn in
            guard let id = turn.first?.id else { return nil }
            return (id, turn)
        }
        let tools: [Tool] = (turns.last ?? []).compactMap { entry in
            if case let .tool(tool) = entry.kind { tool } else { nil }
        }
        let prompt: String? = if case let .user(text, _) = turns.last?.first?.kind {
            // A brief pins its goal alone, not a screenful.
            if let id = turns.last?.first?.id, transcript.brief(id) != nil { BriefOutline(text).goal } else { PastedEnvelope.unwrap(text) }
        } else {
            nil
        }
        ScrollView {
            LazyVStack(alignment: .leading, spacing: filter.promptsOnly ? 8 : 16) {
                if visible.hidden > 0 {
                    Button {
                        shownTurns += 10
                    } label: {
                        Label("Show \(min(visible.hidden, 10)) earlier of \(visible.hidden)", systemImage: "arrow.up.circle")
                            .font(.subheadline.weight(.medium))
                    }
                    .buttonStyle(.glass)
                    .frame(maxWidth: .infinity)
                } else if transcript.truncated {
                    Label("Older messages stay on the computer", systemImage: "clock.arrow.circlepath")
                        .font(.footnote).foregroundStyle(.secondary)
                }
                ForEach(Array(turnRows.enumerated()), id: \.element.id) { offset, row in
                    let isLatest = row.id == turns.last?.first?.id
                    VStack(alignment: .leading, spacing: 0) {
                        if offset > 0 {
                            Divider().padding(.vertical, filter.promptsOnly ? 4 : 8)
                        }
                        // Only the latest turn's prompt pins once scrolled past.
                        ChatTurn(
                            entries: row.entries,
                            promptAbove: !filter.promptsOnly && isLatest ? $promptAbove : nil,
                            brief: transcript.brief(row.id),
                            promptsOnly: filter.promptsOnly,
                            peeked: filter.isPeeked(row.id),
                            onTogglePeek: {
                                withAnimation(.snappy(duration: 0.34)) {
                                    filter.togglePeek(row.id)
                                }
                            },
                            onShowFullHere: {
                                shownTurns = TurnFilterState.expandedShownTurns(
                                    current: shownTurns,
                                    allTurns: turns,
                                    jumpingTo: row.id
                                )
                                withAnimation(.snappy(duration: 0.34)) {
                                    filter.clear()
                                }
                                pendingJump = row.id
                            }
                        )
                    }
                    .id(row.id)
                }
                if !transcript.queued.isEmpty {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Queued").font(.footnote.weight(.semibold)).foregroundStyle(.secondary)
                        ForEach(transcript.queued, id: \.id) { queued in
                            HStack(alignment: .firstTextBaseline, spacing: 8) {
                                Image(systemName: queued.steered ? "arrow.turn.down.right" : queued.held ? "pause.circle" : "clock").foregroundStyle(.secondary)
                                    .accessibilityLabel(queued.steered ? "Steering" : queued.held ? "Held" : "Queued")
                                Text(queued.text).font(.subheadline).lineLimit(3)
                                if !queued.attachments.isEmpty {
                                    Label("\(queued.attachments.count)", systemImage: "photo").font(.caption).foregroundStyle(.secondary)
                                }
                                Spacer()
                                // A steered prompt is already with the agent.
                                if !queued.steered {
                                    Button { sendNow(queued) } label: { Image(systemName: "arrow.up.circle.fill").foregroundStyle(.tint) }
                                        .buttonStyle(.plain)
                                        .accessibilityLabel("Send now")
                                    Button { unqueue(queued) } label: { Image(systemName: "xmark.circle.fill").foregroundStyle(.secondary) }
                                        .buttonStyle(.plain)
                                        .accessibilityLabel("Remove from the queue")
                                }
                            }
                            .padding(10)
                            .background(.fill.quinary, in: .rect(cornerRadius: 12))
                        }
                    }
                }
                ForEach(store.outgoing) { outgoing in
                    OutgoingRow(outgoing: outgoing, computer: computerName, agent: transcript.agent)
                }
                if let problem { Text(problem).font(.footnote).foregroundStyle(.red) }
            }
            .scrollTargetLayout()
            .animation(.snappy, value: store.outgoing)
            .animation(.snappy(duration: 0.34), value: filter)
            .frame(maxWidth: Self.readableWidth, alignment: .leading)
            .frame(maxWidth: .infinity)
            .padding(.horizontal, 20)
            .padding(.top, 10)
            .padding(.bottom, 24)
        }
        .defaultScrollAnchor(.bottom)
        .scrollPosition($position)
        .task(id: pendingJump) {
            guard let target = pendingJump else { return }
            await Task.yield()
            guard !Task.isCancelled else { return }
            withAnimation(.snappy(duration: 0.34)) {
                position.scrollTo(id: target, anchor: .top)
            }
            pendingJump = nil
        }
        .onChange(of: filter.promptsOnly) { _, now in
            if now {
                promptAbove = false
                // Bring the collapsed prompts neatly into view when folding a long thread.
                withAnimation(.snappy(duration: 0.34)) {
                    position.scrollTo(edge: .bottom)
                }
            }
        }
        .onScrollGeometryChange(for: Bool.self) { geometry in
            geometry.contentSize.height - geometry.contentOffset.y - geometry.containerSize.height > 240
        } action: { _, now in
            away = now
        }
        // A snapshot on return, lazy rows measured anew, or the composer
        // changing size can shorten the content under a resting offset;
        // UIKit leaves it past the end until a touch, a blank screen.
        .onScrollGeometryChange(for: Bool.self) { geometry in
            let end = geometry.contentSize.height + geometry.contentInsets.bottom - geometry.containerSize.height
            return geometry.contentOffset.y > max(end, -geometry.contentInsets.top) + 1
        } action: { _, now in
            overshot = now
            if now, !scrolling { position.scrollTo(edge: .bottom) }
        }
        .onScrollPhaseChange { _, phase in
            scrolling = phase != .idle
            if !scrolling, overshot { position.scrollTo(edge: .bottom) }
        }
        // Only a reader's own scroll moves them off the latest; content
        // shrinking under them while idle is repaired above instead.
        .onScrollGeometryChange(for: Bool.self) { geometry in
            let end = geometry.contentSize.height + geometry.contentInsets.bottom - geometry.containerSize.height
            return geometry.contentOffset.y >= end - 8
        } action: { _, now in
            if scrolling || now { atLatest = now }
        }
        // Back in the app, or a fresh snapshot replacing the cached one: a
        // reader at the latest stays there.
        .onChange(of: scenePhase) { _, phase in
            if phase == .active, atLatest { position.scrollTo(edge: .bottom) }
        }
        .onChange(of: store.stale) { _, stale in
            if !stale, atLatest { position.scrollTo(edge: .bottom) }
        }
        // Dragging the conversation puts the keyboard away, as Messages does.
        .scrollDismissesKeyboard(.interactively)
        .coordinateSpace(.named(ChatEffectsModel.space))
        .environment(\.chatEffects, store.effects)
        .modifier(WizzShake(since: store.effects.ledger.playing(at: .now).flatMap { $0.effect == .wizz ? $0.since : nil }))
        // A screen effect plays over the chat alone, never the composer,
        // and takes no touches.
        .overlay {
            let effects = store.effects
            ScreenEffectOverlay(playing: effects.ledger.playing(at: .now)) { [effects] playing in
                effects.target(of: playing)
            }
        }
        // A picture opened full screen swipes through the chat's others.
        .environment(\.chatPictures, transcript.pictures.map(GalleryPicture.init))
        .opacity(store.stale ? 0.6 : 1)
        .safeAreaBar(edge: .top) {
            VStack(spacing: 6) {
                ChatTitleRibbon(
                    title: LockState.shared.hidesNames ? "Sheprd" : name,
                    subtitle: subtitle,
                    canOpenSession: canOpenSession,
                    promptsOnly: filter.promptsOnly,
                    promptCount: promptCount,
                    openSession: openSession,
                    clearFilter: {
                        withAnimation(.snappy(duration: 0.34)) {
                            filter.clear()
                        }
                    }
                )
                if !filter.promptsOnly, let prompt, promptAbove, let latestId = turns.last?.first?.id {
                    PinnedPrompt(text: String(InlineMarkdown.plainSized(prompt).characters)) {
                        withAnimation(.snappy) { position.scrollTo(id: latestId, anchor: .top) }
                    }
                    .transition(.move(edge: .top).combined(with: .opacity))
                }
            }
            .animation(.snappy(duration: 0.34), value: filter.promptsOnly)
            .animation(.snappy, value: !filter.promptsOnly && promptAbove && prompt != nil)
        }
        // A bar, not an inset: what scrolls under it fades as under the
        // system's own toolbars.
        .safeAreaBar(edge: .bottom) {
            // The running turn's status sits where the turn grows, just
            // above the composer, not over the earlier answers at the top.
            VStack(spacing: 8) {
                if away {
                    Button {
                        withAnimation(.snappy) { position.scrollTo(edge: .bottom) }
                    } label: {
                        Image(systemName: "arrow.down").font(.body.weight(.semibold)).frame(width: 36, height: 36)
                    }
                    .buttonStyle(.glass).buttonBorderShape(.circle)
                    .accessibilityLabel("Jump to the latest")
                    .frame(maxWidth: .infinity, alignment: .trailing)
                    .padding(.trailing, 16)
                    .transition(.scale.combined(with: .opacity))
                }
                if let terminal {
                    PromptCard(key: key, agent: transcript.agent, waiting: terminal.needsYou, terminal: terminal.terminalRoute)
                }
                if transcript.status == .working {
                    RunningCapsule(transcript: transcript, tools: tools)
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                }
                Composer(
                    key: key,
                    store: store,
                    transcript: transcript,
                    destination: terminal == nil ? .chat : .terminal,
                    problem: $problem
                )
                .frame(maxWidth: Self.readableWidth + 40)
                .frame(maxWidth: .infinity)
            }
            .animation(.snappy, value: transcript.status == .working)
            .animation(.snappy, value: away)
        }
        .opensScreenLinks(on: key.device)
        .browsesLinks(schemes: MarkdownView.webSchemes)
        .sheet(item: question) { permission in
            QuestionSheet(key: key, permission: permission)
        }
    }

    private var computerName: String {
        model.fleet.machines.first { $0.id == key.device }?.computer.name ?? "your computer"
    }

    /// The oldest question, shown until answered or swiped away.
    private var question: Binding<Permission?> {
        Binding(
            get: { transcript.permissions.first { $0.id != dismissedQuestion } },
            set: { if $0 == nil { dismissedQuestion = transcript.permissions.first?.id } }
        )
    }



    private func unqueue(_ queued: Queued) {
        Task { problem = await model.fleet.send(.unqueue(queued.id), to: key) }
    }

    private func sendNow(_ queued: Queued) {
        Task { problem = await model.fleet.send(.sendNow(queued.id), to: key) }
    }
}

/// Toolbar button that toggles prompts-only mode.
private struct PromptsFilterButton: View {
    let active: Bool
    let count: Int
    let toggle: () -> Void

    var body: some View {
        Button(action: toggle) {
            Image(systemName: "line.3.horizontal.decrease")
                .fontWeight(active ? .semibold : .regular)
        }
        .modifier(ActiveToolbarGlass(active: active))
        .haptic(.selection, trigger: active)
        .accessibilityLabel("My prompts only")
        .accessibilityValue(active ? "On, \(count) prompt\(count == 1 ? "" : "s")" : "Off")
        .accessibilityHint("Collapses responses so only prompts you sent show")
    }
}

private struct ActiveToolbarGlass: ViewModifier {
    let active: Bool

    func body(content: Content) -> some View {
        if active {
            content.buttonStyle(.glassProminent)
        } else {
            content
        }
    }
}

/// Full-width Liquid Glass ribbon directly under the navigation bar: gives the
/// chat title and model line room to breathe, and merges the active prompts
/// filter chip inline so filtering adds no third bar.
private struct ChatTitleRibbon: View {
    let title: String
    let subtitle: String
    let canOpenSession: Bool
    let promptsOnly: Bool
    let promptCount: Int
    let openSession: () -> Void
    let clearFilter: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            Button(action: openSession) {
                VStack(alignment: .leading, spacing: 1) {
                    HStack(spacing: 4) {
                        Text(title).font(.subheadline.weight(.semibold)).lineLimit(1)
                        if canOpenSession {
                            Image(systemName: "chevron.down.circle.fill")
                                .font(.caption)
                                .foregroundStyle(.tertiary)
                        }
                    }
                    if !subtitle.isEmpty {
                        Text(subtitle)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .disabled(!canOpenSession)
            .accessibilityHint(canOpenSession ? "Shows the session's settings" : "")

            if promptsOnly {
                Button(action: clearFilter) {
                    HStack(spacing: 4) {
                        Text("\(promptCount) prompt\(promptCount == 1 ? "" : "s")")
                        Image(systemName: "xmark").font(.caption2.weight(.bold))
                    }
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.white)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 6)
                    .background(.tint, in: .capsule)
                }
                .buttonStyle(.plain)
                .transition(.scale(scale: 0.85).combined(with: .opacity))
                .accessibilityLabel("Showing \(promptCount) prompt\(promptCount == 1 ? "" : "s"). Show all messages")
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .glassEffect(.regular, in: .rect(cornerRadius: 18))
        .frame(maxWidth: ChatDocument.readableWidth + 40)
        .padding(.horizontal, 14)
        .padding(.top, 4)
        .padding(.bottom, 2)
    }
}

/// The latest prompt, three lines at most, over the top of the chat while
/// its heading is scrolled away; a tap scrolls back to it.
struct PinnedPrompt: View {
    static let anchor = "latest-prompt"
    let text: String
    let reveal: () -> Void

    var body: some View {
        Button(action: reveal) {
            HStack(alignment: .top, spacing: 10) {
                Rectangle().fill(.tint).frame(width: 3)
                Text(text).font(.subheadline.weight(.semibold)).lineLimit(3)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .fixedSize(horizontal: false, vertical: true)
            .padding(.horizontal, 14).padding(.vertical, 10)
            .glassEffect(.regular, in: .rect(cornerRadius: 16))
        }
        .buttonStyle(.plain)
        .frame(maxWidth: ChatDocument.readableWidth + 40)
        .padding(.horizontal, 14)
        .padding(.bottom, 2)
        .accessibilityLabel("Your prompt: \(text)")
        .accessibilityHint("Scrolls to it")
    }
}

/// A prompt on its way, until the transcript shows it as a turn or queued.
private struct OutgoingRow: View {
    let outgoing: Outgoing
    let computer: String
    let agent: String

    var body: some View {
        VStack(alignment: .trailing, spacing: 4) {
            VStack(alignment: .leading, spacing: 6) {
                if !outgoing.text.isEmpty { Text(outgoing.text).lineLimit(6) }
                let pictures = outgoing.uploads.count
                if pictures > 0 {
                    Label("\(pictures) picture\(pictures == 1 ? "" : "s")", systemImage: "photo")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }
            .padding(.horizontal, 14).padding(.vertical, 10)
            .background(.tint.opacity(0.18), in: .rect(cornerRadius: 16))
            ThinProgress(value: outgoing.progress).frame(width: 140)
            Text(stageLine).font(.caption).foregroundStyle(.secondary).monospacedDigit()
        }
        .frame(maxWidth: .infinity, alignment: .trailing)
        .transition(.move(edge: .bottom).combined(with: .opacity))
        .accessibilityElement(children: .combine)
    }

    private var stageLine: String {
        switch outgoing.stage {
        case .uploading:
            let pictures = outgoing.uploads.count == 1 ? "picture" : "pictures"
            return "Uploading \(pictures)… \(Int((outgoing.progress / 0.8 * 100).rounded()))%"
        case .toComputer: return "Sending to \(computer)…"
        case .toAgent: return "Sending to \(agent)…"
        }
    }
}

/// What the agent is doing now, pinned above the composer.
private struct RunningCapsule: View {
    let transcript: Transcript
    let tools: [Tool]

    var body: some View {
        let current = tools.last { $0.status == .inProgress } ?? tools.last { $0.status == .pending }
        HStack(spacing: 8) {
            ProgressView().controlSize(.small)
            Text(current?.title ?? "Thinking").font(.subheadline.weight(.medium)).lineLimit(1)
            Spacer()
            if let started = transcript.turn?.startedMs {
                Text(timerInterval: Date(timeIntervalSince1970: Double(started) / 1000)...Date.distantFuture, countsDown: false)
                    .font(.subheadline.monospacedDigit()).foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 14).padding(.vertical, 9)
        .glassEffect(.regular, in: .capsule)
        .frame(maxWidth: ChatDocument.readableWidth)
        .padding(.horizontal, 16)
    }
}
