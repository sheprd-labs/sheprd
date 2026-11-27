import SheprdRemoteCore
import SwiftUI

/// One turn of a conversation: the prompt as its heading, the steps folded
/// to one line, the answer, then the files it changed. Every turn reads the
/// same, so the conversation scrolls back through its history as a messages
/// app does.
struct ChatTurn: View {
    let entries: ArraySlice<Entry>
    /// Set on the latest turn: whether its prompt has scrolled up past the
    /// top of the chat, so the chat pins it there.
    var promptAbove: Binding<Bool>?
    /// Set when another agent wrote the prompt: it shows as a card.
    var brief: Brief?
    /// Whether the session is filtered to only show sent prompts.
    var promptsOnly = false
    /// Whether this turn is temporarily expanded inline during prompts-only mode.
    var peeked = false
    /// Toggles inline peek when the user taps the prompt row in prompts-only mode.
    var onTogglePeek: (() -> Void)?
    /// Exits prompts-only mode and anchors the full session on this turn.
    var onShowFullHere: (() -> Void)?

    /// A transcript's entries split where each prompt starts; whatever came
    /// before the first prompt is a turn of its own, without a heading.
    static func split(_ entries: [Entry]) -> [ArraySlice<Entry>] {
        ChatTurns.split(entries)
    }

    var body: some View {
        let tools = entries.compactMap { if case let .tool(tool) = $0.kind { tool } else { nil } }
        let prompt: (text: String, rest: ArraySlice<Entry>)? = if case let .user(text, _) = entries.first?.kind {
            (text, entries.dropFirst())
        } else {
            nil
        }
        let showBody = !promptsOnly || peeked
        let collapsedMeta = promptsOnly && !peeked ? ChatTurns.collapsedSummary(for: entries) : nil

        VStack(alignment: .leading, spacing: showBody ? 16 : 0) {
            if let prompt {
                promptSection(prompt.text, collapsedMeta: collapsedMeta)
            }
            if showBody {
                VStack(alignment: .leading, spacing: 16) {
                    if !tools.isEmpty {
                        DisclosureGroup {
                            VStack(alignment: .leading, spacing: 8) {
                                ForEach(tools, id: \.callId) { ToolRow(tool: $0) }
                            }
                            .padding(.top, 6)
                        } label: {
                            Label(Self.stepsLine(tools), systemImage: "list.bullet.indent")
                                .font(.subheadline).foregroundStyle(.secondary)
                        }
                        Divider()
                    }
                    ForEach(Array(prompt?.rest ?? entries)) { entry in
                        if case .tool = entry.kind {} else { EntryView(entry: entry) }
                    }
                    ForEach(tools.flatMap(Self.diffs), id: \.path) { diff in
                        DiffBlock(path: diff.path, old: diff.old, new: diff.new)
                    }
                    if promptsOnly, peeked, let onShowFullHere {
                        Button(action: onShowFullHere) {
                            Label("Show full session here", systemImage: "arrow.up.left.and.arrow.down.right")
                                .font(.caption.weight(.semibold))
                        }
                        .buttonStyle(.bordered)
                        .buttonBorderShape(.capsule)
                        .tint(.accentColor)
                    }
                }
                .padding(.top, prompt != nil && promptsOnly ? 4 : 0)
                .clipped()
                .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
    }

    @ViewBuilder
    private func promptSection(_ text: String, collapsedMeta: String?) -> some View {
        if promptsOnly {
            Button {
                onTogglePeek?()
            } label: {
                HStack(alignment: .center, spacing: 10) {
                    VStack(alignment: .leading, spacing: 5) {
                        heading(text, selectable: false)
                        if let collapsedMeta {
                            Text(collapsedMeta)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                                .padding(.leading, 13)
                                .transition(.opacity)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    Image(systemName: "chevron.right")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.tertiary)
                        .rotationEffect(.degrees(peeked ? 90 : 0))
                }
                .padding(.vertical, 4)
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .accessibilityHint(peeked ? "Collapses this turn" : "Shows this turn's response")
        } else if let promptAbove {
            heading(text, selectable: true)
                .id(PinnedPrompt.anchor)
                // Below the viewport (reading earlier turns) is not
                // "above": only a prompt scrolled past pins.
                .onGeometryChange(for: Bool.self) { $0.frame(in: .scrollView).maxY < 0 } action: { promptAbove.wrappedValue = $0 }
        } else {
            heading(text, selectable: true)
        }
    }

    @ViewBuilder
    private func heading(_ text: String, selectable: Bool) -> some View {
        if let brief {
            BriefCard(text: text, brief: brief)
        } else {
            PromptHeading(text: PastedEnvelope.unwrap(text), selectable: selectable)
        }
    }

    static func stepsLine(_ tools: [Tool]) -> String {
        ChatTurns.stepsLine(tools)
    }

    private static func diffs(_ tool: Tool) -> [(path: String, old: String?, new: String)] {
        tool.content.compactMap { if case let .diff(path, old, new) = $0 { (path, old, new) } else { nil } }
    }
}

/// A prompt as its turn's heading. A picture this app sends into a
/// terminal reaches the agent as the path its computer stored it at; that
/// path shows as a picture mark. Any other path stays as typed.
private struct PromptHeading: View {
    let text: String
    var selectable = true

    var body: some View {
        let (words, pictures) = PromptPictures.split(text)
        HStack(alignment: .top, spacing: 10) {
            Rectangle().fill(.tint).frame(width: 3)
            VStack(alignment: .leading, spacing: 6) {
                if pictures > 0 {
                    Label(pictures == 1 ? "Picture" : "\(pictures) pictures", systemImage: "photo")
                        .font(.subheadline).foregroundStyle(.secondary)
                }
                if !words.isEmpty {
                    let heading = SizedPlain(text: words, style: .title3).font(.title3.weight(.semibold))
                    if selectable {
                        heading.textSelection(.enabled)
                    } else {
                        heading
                    }
                }
            }
        }
        .fixedSize(horizontal: false, vertical: true)
    }
}
