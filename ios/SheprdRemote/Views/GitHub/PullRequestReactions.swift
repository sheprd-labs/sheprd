import SheprdGitHub
import SwiftUI

/// Emoji reactions under a pull request's description, with a picker to
/// add or toggle any of GitHub's eight reactions when the signed-in account
/// may react.
struct PullRequestReactions: View {
    let reactions: [PullRequestDetail.ReactionGroup]
    let canReact: Bool
    let pending: Set<PullRequestDetail.ReactionContent>
    let error: String?
    let onToggle: ((PullRequestDetail.ReactionContent) -> Void)?
    @State private var taps = 0

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            PillFlow(spacing: 6) {
                ForEach(reactions) { group in
                    if canReact, let onToggle {
                        let busy = pending.contains(group.content)
                        Button {
                            taps += 1
                            onToggle(group.content)
                        } label: {
                            pill(group)
                        }
                        .buttonStyle(.plain)
                        .disabled(busy)
                        .opacity(busy ? 0.55 : 1)
                        .accessibilityLabel("\(group.content.label), \(group.count)")
                        .accessibilityAddTraits(group.viewerHasReacted ? .isSelected : [])
                        .accessibilityHint(group.viewerHasReacted ? "Removes your reaction" : "Adds your reaction")
                    } else {
                        pill(group)
                            .accessibilityElement(children: .ignore)
                            .accessibilityLabel("\(group.content.label), \(group.count)")
                    }
                }
                if canReact, let onToggle {
                    picker(onToggle)
                }
            }
            .haptic(.selection, trigger: taps)
            if let error {
                Text(error).font(.caption).foregroundStyle(.red)
            }
        }
        .padding(.top, 2)
    }

    private func pill(_ group: PullRequestDetail.ReactionGroup) -> some View {
        HStack(spacing: 4) {
            Text(group.content.emoji)
            Text(verbatim: "\(group.count)").font(.caption.weight(.medium).monospacedDigit())
        }
        .font(.caption)
        .foregroundStyle(group.viewerHasReacted ? AnyShapeStyle(.tint) : AnyShapeStyle(.primary))
        .padding(.horizontal, 9).padding(.vertical, 4)
        .background(
            group.viewerHasReacted ? AnyShapeStyle(.tint.opacity(0.16)) : AnyShapeStyle(.fill.tertiary),
            in: .capsule
        )
        .overlay {
            Capsule().strokeBorder(
                group.viewerHasReacted ? AnyShapeStyle(.tint.opacity(0.45)) : AnyShapeStyle(.clear),
                lineWidth: 1
            )
        }
    }

    private func picker(_ onToggle: @escaping (PullRequestDetail.ReactionContent) -> Void) -> some View {
        Menu {
            ForEach(PullRequestDetail.ReactionContent.allCases) { content in
                let reacted = reactions.contains { $0.content == content && $0.viewerHasReacted }
                Button {
                    taps += 1
                    onToggle(content)
                } label: {
                    if reacted {
                        Label("\(content.emoji) \(content.label)", systemImage: "checkmark")
                    } else {
                        Text("\(content.emoji) \(content.label)")
                    }
                }
                .disabled(pending.contains(content))
            }
        } label: {
            HStack(spacing: 3) {
                Image(systemName: "face.smiling")
                Image(systemName: "plus").font(.system(size: 9, weight: .bold))
            }
            .font(.caption.weight(.medium))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 9).padding(.vertical, 5)
            .background(.fill.tertiary, in: .capsule)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Add reaction")
    }
}
