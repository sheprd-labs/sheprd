import SheprdRemoteCore
import SwiftUI

/// Static helpers and flattened row views for a chat turn (`ChatTurnRow`).
/// Each part of a turn is rendered as a direct child of `ChatDocument`'s
/// `LazyVStack` so long coding turns stay lazy during navigation pushes and
/// scrolling.
enum ChatTurn {
    /// A transcript's entries split where each prompt starts; whatever came
    /// before the first prompt is a turn of its own, without a heading.
    static func split(_ entries: [Entry]) -> [ArraySlice<Entry>] {
        ChatTurns.split(entries)
    }

    static func stepsLine(_ tools: [Tool]) -> String {
        ChatTurns.stepsLine(tools)
    }
}

/// One turn's prompt heading, brief card, or collapsible prompts-only header.
struct ChatPromptRow: View {
    let text: String
    /// Set on the latest turn: whether its prompt has scrolled up past the
    /// top of the chat, so the chat pins it there.
    var promptAbove: Binding<Bool>?
    /// Set when another agent wrote the prompt: it shows as a card.
    var brief: Brief?
    var promptsOnly = false
    var peeked = false
    var collapsedMeta: String?
    var onTogglePeek: (() -> Void)?

    var body: some View {
        if promptsOnly {
            Button {
                onTogglePeek?()
            } label: {
                HStack(alignment: .center, spacing: 10) {
                    VStack(alignment: .leading, spacing: 5) {
                        heading(selectable: false)
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
            heading(selectable: true)
                // Below the viewport (reading earlier turns) is not
                // "above": only a prompt scrolled past pins.
                .onGeometryChange(for: Bool.self) { $0.frame(in: .scrollView).maxY < 0 } action: { promptAbove.wrappedValue = $0 }
        } else {
            heading(selectable: true)
        }
    }

    @ViewBuilder
    private func heading(selectable: Bool) -> some View {
        if let brief {
            BriefCard(text: text, brief: brief)
        } else {
            PromptHeading(text: PastedEnvelope.unwrap(text), selectable: selectable)
        }
    }
}

/// Folded tool calls for a turn. Uses an explicit `expanded` gate instead of
/// `DisclosureGroup` so collapsed turns with dozens of tool calls build zero
/// `ToolRow` views until tapped.
struct ChatStepsRow: View {
    let tools: [Tool]
    @State private var expanded = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button {
                withAnimation(.snappy) { expanded.toggle() }
            } label: {
                HStack(spacing: 8) {
                    Label(ChatTurn.stepsLine(tools), systemImage: "list.bullet.indent")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                    Spacer(minLength: 4)
                    Image(systemName: "chevron.right")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.tertiary)
                        .rotationEffect(.degrees(expanded ? 90 : 0))
                }
                .padding(.vertical, 4)
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .accessibilityHint(expanded ? "Hides steps" : "Shows steps")

            if expanded {
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(tools, id: \.callId) { ToolRow(tool: $0) }
                }
                .padding(.top, 6)
                .padding(.bottom, 8)
            }

            Divider().padding(.top, 8)
        }
    }
}

/// Action shown at the foot of an inline-peeked turn in prompts-only mode.
struct ChatShowFullRow: View {
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Label("Show full session here", systemImage: "arrow.up.left.and.arrow.down.right")
                .font(.caption.weight(.semibold))
        }
        .buttonStyle(.bordered)
        .buttonBorderShape(.capsule)
        .tint(.accentColor)
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
