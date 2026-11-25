import ActivityKit
import SheprdRemoteCore
import SwiftUI
import WidgetKit

/// A chat's Live Activity on the lock screen, in the style chosen in
/// Settings (`LiveActivityStyle`). Compiled into the widget, which draws
/// it, and the app, whose Settings previews each style with it.
struct LockScreenView: View {
    let facts: ChatActivityFacts
    let state: ChatActivityState
    var style: LiveActivityStyle = LiveActivityStyleSetting.current

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            switch style {
            case .private, .minimal: MinimalCard(facts: facts, state: state, style: style)
            case .standard: StandardCard(facts: facts, state: state, complete: false)
            case .complete: StandardCard(facts: facts, state: state, complete: true)
            case .geek: GeekCard(facts: facts, state: state)
            }
            if style != .standard && style != .complete {
                AgentProgressLine(state: state, style: style)
            }
        }
        .padding(14)
        .background(state.phase == .question ? Palette.question.opacity(0.18) : Color.clear)
    }
}

private struct AgentProgressLine: View {
    let state: ChatActivityState
    let style: LiveActivityStyle

    var body: some View {
        if let agents = state.agents {
            VStack(alignment: .leading, spacing: 2) {
                Text(agents.label).font(.caption.weight(.semibold))
                if style != .private, let latest = agents.latest {
                    Text(latest).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
            }
        }
    }
}

/// What the first line names: the chat, or its workspace when it has no
/// title (or a computer left it out).
private func name(_ facts: ChatActivityFacts) -> String {
    facts.chatTitle.isEmpty ? facts.workspace : facts.chatTitle
}

/// Whether the activity may offer links into a chat: the test opens none.
private func opensChat(_ facts: ChatActivityFacts) -> Bool {
    !SelfTestActivity.isTest(facts)
}

/// The phase in a few words.
func phaseHeadline(_ phase: ChatActivityState.Phase) -> String {
    switch phase {
    case .working: "Working"
    case .question: "Waiting for you"
    case .finished: "Finished"
    case .failed: "Stopped"
    }
}

// MARK: - Private and Minimal

private struct MinimalCard: View {
    let facts: ChatActivityFacts
    let state: ChatActivityState
    let style: LiveActivityStyle

    var body: some View {
        HStack(spacing: 10) {
            AgentBadge(agent: facts.agent, size: 30)
            VStack(alignment: .leading, spacing: 1) {
                Text(style == .private ? facts.agent : name(facts))
                    .font(.headline)
                    .lineLimit(1)
                Text(style == .private || facts.workspace == facts.computer ? facts.computer : "\(facts.computer) \u{b7} \(facts.workspace)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 6)
            trailing
        }
    }

    @ViewBuilder
    private var trailing: some View {
        switch state.phase {
        case .working:
            HStack(spacing: 6) {
                PhaseMark(phase: .working, size: 7)
                Elapsed(since: state.startedAt)
                    .font(.subheadline.weight(.semibold))
                    .frame(maxWidth: 64, alignment: .trailing)
            }
        case .question: StatusPill(text: "Needs you", color: Palette.question, filled: true)
        case .finished: StatusPill(text: "Done", color: Palette.working, filled: false)
        case .failed: StatusPill(text: "Stopped", color: Palette.failed, filled: false)
        }
    }
}

private struct StatusPill: View {
    let text: String
    let color: Color
    let filled: Bool

    var body: some View {
        Text(text)
            .font(.subheadline.weight(.semibold))
            .padding(.horizontal, 12)
            .padding(.vertical, 5)
            .background(filled ? color : Color.white.opacity(0.16), in: Capsule())
            .foregroundStyle(filled ? Color.black : (color == Palette.failed ? color : Color.white))
    }
}

// MARK: - Standard and Complete

private struct StandardCard: View {
    let facts: ChatActivityFacts
    let state: ChatActivityState
    let complete: Bool

    private var progressHeadline: String? {
        guard state.phase == .working || state.phase == .finished else { return nil }
        return state.agents?.label
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 9) {
            HStack(spacing: 8) {
                AgentBadge(agent: facts.agent, size: 24)
                Text(name(facts))
                    .font(.subheadline.weight(.semibold))
                    .lineLimit(1)
                Spacer(minLength: 4)
                Text(facts.computer)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            if complete, state.agents == nil, state.phase == .working || state.phase == .question {
                ForEach(Array(state.plainPrevious.suffix(2).enumerated()), id: \.offset) { _, step in
                    Label(step, systemImage: "checkmark")
                        .font(.caption)
                        .lineLimit(1)
                        .foregroundStyle(.white.opacity(0.45))
                }
            }
            HStack(alignment: .center, spacing: 10) {
                PhaseMark(phase: state.phase)
                VStack(alignment: .leading, spacing: 2) {
                    HStack {
                        Text(progressHeadline ?? phaseHeadline(state.phase)).font(.headline)
                        if state.phase == .working {
                            Spacer(minLength: 4)
                            Elapsed(since: state.startedAt)
                                .font(.caption.weight(.semibold))
                                .foregroundStyle(.secondary)
                                .frame(maxWidth: 64, alignment: .trailing)
                        }
                    }
                    detail
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(0.8))
                        .lineLimit(2)
                }
                if state.phase == .finished, opensChat(facts) {
                    Spacer(minLength: 4)
                    Link(destination: facts.link.url) {
                        Text("Reply")
                            .font(.subheadline.weight(.semibold))
                            .padding(.horizontal, 14)
                            .padding(.vertical, 6)
                            .background(.white.opacity(0.18), in: Capsule())
                    }
                }
            }
            if progressHeadline == nil, state.agents != nil {
                AgentProgressLine(state: state, style: complete ? .complete : .standard)
            }
            if opensChat(facts) {
                AnswerButtons(facts: facts, state: state)
            }
            if state.phase == .working || state.phase == .finished {
                TurnProgress(state: state)
                if state.agents == nil { footer }
            }
        }
    }

    @ViewBuilder
    private var detail: some View {
        if progressHeadline != nil, let latest = state.agents?.latest {
            Text(latest)
        } else {
            ordinaryDetail
        }
    }

    @ViewBuilder
    private var ordinaryDetail: some View {
        switch state.phase {
        case .working:
            if let icon = state.icon {
                Label(state.plainStep, systemImage: icon.symbol)
            } else {
                Text(state.plainStep)
            }
        case .question: Text(state.question ?? state.plainStep)
        case .finished, .failed: Text(state.summary ?? state.plainStep)
        }
    }

    @ViewBuilder
    private var footer: some View {
        let plan = state.phase == .working ? state.planLabel : nil
        let counts = complete ? state.counts : nil
        if plan != nil || counts != nil {
            HStack {
                if let plan { Text(plan) }
                Spacer(minLength: 4)
                if let counts { Text(counts) }
            }
            .font(.caption)
            .foregroundStyle(.secondary)
        }
        if complete, state.phase == .working, state.working + state.waiting > 1 {
            Text(fleetLine(state, computer: facts.computer))
                .font(.caption2)
                .foregroundStyle(.white.opacity(0.5))
        }
    }
}

/// `studio: 2 working · 1 waiting`, this computer's chats.
func fleetLine(_ state: ChatActivityState, computer: String) -> String {
    var parts = ["\(max(state.working, 1)) working"]
    if state.waiting > 0 { parts.append("\(state.waiting) waiting") }
    return "\(computer): " + parts.joined(separator: " \u{b7} ")
}

// MARK: - Geek

private struct GeekCard: View {
    let facts: ChatActivityFacts
    let state: ChatActivityState

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                AgentBadge(agent: facts.agent, size: 20)
                Text(facts.workspace == facts.computer ? facts.computer : "\(facts.computer):\(facts.workspace)")
                    .font(.system(.caption, design: .monospaced).weight(.semibold))
                    .lineLimit(1)
                if !facts.chatTitle.isEmpty {
                    Text(facts.chatTitle).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
                Spacer(minLength: 4)
                if state.phase == .working {
                    Elapsed(since: state.startedAt)
                        .font(.system(.caption, design: .monospaced))
                        .frame(maxWidth: 64, alignment: .trailing)
                }
            }
            VStack(alignment: .leading, spacing: 3) {
                ForEach(Array(state.previous.suffix(2).enumerated()), id: \.offset) { _, step in
                    HStack(spacing: 6) {
                        Text("✓").foregroundStyle(Palette.working)
                        Text(step).foregroundStyle(.white.opacity(0.45)).lineLimit(1)
                    }
                }
                HStack(alignment: .top, spacing: 6) {
                    Text(mark).foregroundStyle(Palette.phase(state.phase))
                    // The answer buttons below answer this question, so it
                    // shows whole, never the step, which can move on while
                    // the question waits.
                    if state.phase == .question, let question = state.question {
                        Text(question).lineLimit(3)
                    } else {
                        Text(state.step).lineLimit(1)
                    }
                }
                if let summary = state.summary, state.phase == .finished || state.phase == .failed {
                    Text("# \(summary)").foregroundStyle(.secondary).lineLimit(2)
                }
            }
            .font(.system(.footnote, design: .monospaced))
            .padding(8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.black.opacity(0.55), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
            if opensChat(facts) {
                AnswerButtons(facts: facts, state: state)
            }
            let parts = [state.counts, state.planTotal.map { "plan \(state.planDone ?? 0)/\($0)" }].compactMap(\.self)
            if !parts.isEmpty {
                Text(parts.joined(separator: " \u{b7} "))
                    .font(.system(.caption2, design: .monospaced))
                    .foregroundStyle(.secondary)
            }
        }
    }

    private var mark: String {
        switch state.phase {
        case .working: "$"
        case .question: "?"
        case .finished: "✓"
        case .failed: "✗"
        }
    }
}

// MARK: - The Dynamic Island

/// The island's parts in each style; the widget puts them in its regions,
/// and Settings' preview stacks them.
enum IslandParts {
    @ViewBuilder
    static func compactLeading(_ state: ChatActivityState, facts: ChatActivityFacts, style: LiveActivityStyle) -> some View {
        switch style {
        case .private, .minimal:
            AgentBadge(agent: facts.agent, size: 18)
        case .geek:
            Text("$").font(.system(.caption, design: .monospaced).bold()).foregroundStyle(Palette.working)
        case .standard, .complete:
            HStack(spacing: 4) {
                if let icon = state.icon, state.phase == .working {
                    Image(systemName: icon.symbol).foregroundStyle(Palette.working)
                } else {
                    Circle().fill(Palette.working).frame(width: 8, height: 8)
                }
                Text("\(max(state.working, 1))")
                    .font(.caption2.weight(.bold))
                    .monospacedDigit()
            }
        }
    }

    @ViewBuilder
    static func compactTrailing(_ state: ChatActivityState) -> some View {
        if state.phase == .question || state.waiting > 0 {
            HStack(spacing: 3) {
                Circle().fill(Palette.question).frame(width: 8, height: 8)
                Text("\(max(state.waiting, 1))")
                    .font(.caption2.weight(.bold))
                    .monospacedDigit()
            }
            .foregroundStyle(Palette.question)
        } else if let agents = state.agents {
            Text("\(agents.done)/\(agents.total)").font(.caption2.bold()).monospacedDigit()
                .accessibilityLabel(agents.label)
        } else if state.phase == .working {
            Elapsed(since: state.startedAt)
                .font(.caption2.weight(.semibold))
                .frame(maxWidth: 40)
        } else {
            PhaseMark(phase: state.phase, size: 8)
        }
    }

    static func title(_ state: ChatActivityState, facts: ChatActivityFacts, style: LiveActivityStyle) -> String {
        switch style {
        case .private, .minimal, .geek: name(facts)
        case .standard, .complete:
            switch state.phase {
            case .working: "\(facts.agent) is working"
            case .question: "\(facts.agent) needs an answer"
            case .finished: "\(facts.agent) finished"
            case .failed: "\(facts.agent) stopped"
            }
        }
    }

    static func subtitle(facts: ChatActivityFacts, style: LiveActivityStyle) -> String {
        switch style {
        case .private, .minimal: facts.computer
        case .geek: facts.workspace == facts.computer ? facts.computer : "\(facts.computer):\(facts.workspace)"
        case .standard, .complete: facts.chatTitle.isEmpty ? facts.place : "\(facts.chatTitle) \u{b7} \(facts.computer)"
        }
    }

    /// The expanded island's lower half.
    @ViewBuilder
    static func bottom(_ state: ChatActivityState, facts: ChatActivityFacts, style: LiveActivityStyle) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            switch style {
            case .private, .minimal:
                Text(phaseHeadline(state.phase))
                    .font(.subheadline)
                    .foregroundStyle(Palette.phase(state.phase))
                // Minimal offers answers here, so the question shows with them.
                if style == .minimal, state.phase == .question, let question = state.question {
                    Text(question).font(.subheadline).lineLimit(3)
                }
            case .standard, .complete:
                Group {
                    switch state.phase {
                    case .working:
                        if let icon = state.icon {
                            Label(state.plainStep, systemImage: icon.symbol)
                        } else {
                            Text(state.plainStep)
                        }
                    case .question: Text(state.question ?? state.plainStep)
                    case .finished, .failed: Text(state.summary ?? state.plainStep)
                    }
                }
                .font(.subheadline)
                .lineLimit(2)
                if style == .complete, state.phase == .working, let fraction = state.planFraction {
                    ProgressView(value: fraction).tint(Palette.working)
                    HStack {
                        Text(state.planLabel ?? "")
                        Spacer()
                        Text(state.counts ?? "")
                    }
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
            case .geek:
                Text(state.phase == .question ? "? \(state.question ?? state.step)" : "$ \(state.step)")
                    .font(.system(.footnote, design: .monospaced))
                    .lineLimit(3)
                    .padding(8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(.white.opacity(0.08), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                if let counts = state.counts {
                    Text(counts).font(.system(.caption2, design: .monospaced)).foregroundStyle(.secondary)
                }
            }
            AgentProgressLine(state: state, style: style)
            if style != .private, opensChat(facts) {
                AnswerButtons(facts: facts, state: state)
            }
        }
    }
}
