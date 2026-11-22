import SheprdRemoteCore
import SwiftUI

/// What a terminal agent waits on, answered here rather than in its
/// terminal: its question or permission prompt as a card, each choice a
/// button, a choice that takes words a field. The computer reads the
/// question off the pane's screen and presses the keys a person would, so a
/// set of questions goes one at a time and ends on the agent's own review,
/// as it does at the desk. When no question can be read, it points to the
/// terminal.
struct PromptCard: View {
    @Environment(AppModel.self) private var model
    let key: PaneKey
    let agent: String
    /// Herdr says the agent waits on the person.
    let waiting: Bool
    let terminal: Route?
    @State private var prompt: TermPrompt?
    @State private var words = ""
    @State private var sending = false
    @State private var problem: String?
    @State private var answered = 0

    /// Choices are rows, not pills.
    private static let radius: CGFloat = 8

    var body: some View {
        Group {
            if let prompt {
                card(prompt)
            } else if waiting, let terminal {
                banner(terminal)
            }
        }
        .animation(.snappy, value: prompt)
        .haptic(.selection, trigger: answered)
        .task(id: waiting) {
            while !Task.isCancelled {
                await refresh()
                try? await Task.sleep(for: waiting ? .seconds(1.5) : .seconds(4))
            }
        }
    }

    private func card(_ prompt: TermPrompt) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(Array(prompt.question.enumerated()), id: \.offset) { index, line in
                    Text(line)
                        .font(index == prompt.question.count - 1 ? .headline : .subheadline)
                        .foregroundStyle(index == prompt.question.count - 1 ? .primary : .secondary)
                        .lineLimit(4)
                        .textSelection(.enabled)
                }
            }
            ScrollView {
                VStack(spacing: 8) {
                    ForEach(prompt.choices) { choice in
                        if choice.takesWords {
                            wordsField(choice, of: prompt)
                        } else {
                            Button { choose(choice, of: prompt, text: nil) } label: {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(choice.label).font(.body.weight(.medium))
                                    if let detail = choice.detail {
                                        Text(detail).font(.footnote).foregroundStyle(.secondary)
                                    }
                                }
                                .multilineTextAlignment(.leading)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .padding(.horizontal, 12).padding(.vertical, 10)
                                .background(.fill.tertiary, in: .rect(cornerRadius: Self.radius))
                                .contentShape(.rect(cornerRadius: Self.radius))
                            }
                            .buttonStyle(.plain)
                            .disabled(sending)
                        }
                    }
                }
            }
            .scrollBounceBehavior(.basedOnSize)
            .frame(maxHeight: 320)
            HStack {
                if let problem { Text(problem).font(.footnote).foregroundStyle(.red) }
                Spacer()
                if let terminal {
                    RouteLink(terminal) { Text("Terminal").font(.footnote) }
                }
            }
        }
        .padding(14)
        .glassEffect(.regular, in: .rect(cornerRadius: 16))
        .padding(.horizontal, 16)
        .transition(.move(edge: .bottom).combined(with: .opacity))
    }

    private func wordsField(_ choice: TermPrompt.Choice, of prompt: TermPrompt) -> some View {
        HStack(spacing: 8) {
            TextField(choice.label, text: $words, axis: .vertical)
                .lineLimit(1 ... 4)
                .padding(.horizontal, 12).padding(.vertical, 9)
                .background(.fill.tertiary, in: .rect(cornerRadius: Self.radius))
            Button { choose(choice, of: prompt, text: words) } label: {
                Image(systemName: "arrow.up").font(.body.weight(.semibold)).frame(width: 28, height: 28)
            }
            .buttonStyle(.glassProminent).buttonBorderShape(.circle)
            .disabled(sending || words.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            .accessibilityLabel("Answer")
        }
    }

    private func banner(_ terminal: Route) -> some View {
        HStack(spacing: 10) {
            Image(systemName: "hand.raised.fill").foregroundStyle(Home.AgentStatus.blocked.color)
            Text("\(agent.isEmpty ? "The agent" : agent) is waiting for you in the terminal")
                .font(.subheadline.weight(.medium))
            Spacer(minLength: 0)
            RouteLink(terminal) { Text("Open").font(.subheadline.weight(.semibold)) }
                .buttonStyle(.glass)
        }
        .padding(.leading, 14).padding(.trailing, 6).padding(.vertical, 6)
        .glassEffect(.regular, in: .capsule)
        .padding(.horizontal, 16)
        .accessibilityElement(children: .combine)
    }

    /// Shows what the computer reads now: no question clears the card. A
    /// read that fails keeps what is shown, rather than hiding a question
    /// still waiting.
    private func refresh() async {
        let pane = key.pane
        let found: TermPrompt?
        do {
            found = try await model.fleet.call(key.device) { client, token in
                try await client.termPrompt(pane: pane, bearer: token)
            }
        } catch {
            return
        }
        if found != prompt {
            prompt = found
            problem = nil
        }
    }

    private func choose(_ choice: TermPrompt.Choice, of shown: TermPrompt, text: String?) {
        let pane = key.pane
        sending = true
        Task {
            defer { sending = false }
            do {
                try await model.fleet.call(key.device) { client, token in
                    try await client.termChoose(choice, of: shown, text: text, pane: pane, bearer: token)
                }
                answered += 1
                words = ""
                problem = nil
                // The next question of a set, or none, shows at once.
                try? await Task.sleep(for: .milliseconds(350))
                await refresh()
            } catch {
                problem = error.localizedDescription
                await refresh()
            }
        }
    }
}
