import SwiftUI
import SheprdRemoteCore

/// A permission request without choices, in a half-height sheet: what the
/// agent asks for, shortened to a peek, and the answers pinned in thumb
/// reach. A long list of answers (an extension's pick list) scrolls with
/// the request instead, so none falls off the screen. A command shows as
/// code; tapping the peek reads it in full at
/// full height, with copy.
struct PermissionRequest: View {
    let permission: Permission
    let answer: (PermissionOption) -> Void
    @State private var expanded = false
    @State private var detent: PresentationDetent = .medium

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                if let command = permission.command {
                    header
                    commandCard(command)
                } else {
                    titleText
                }
                if !pinsAnswers { answers.padding(.horizontal, -24) }
            }
            .padding(24)
        }
        .safeAreaInset(edge: .bottom) { if pinsAnswers { answers } }
        .presentationDetents([.medium, .large], selection: $detent)
        .presentationContentInteraction(.resizes)
        .presentationDragIndicator(.visible)
        .onChange(of: detent) { _, now in expanded = now == .large }
    }

    /// Approvals stay in thumb reach; a longer list scrolls.
    private var pinsAnswers: Bool { permission.options.count <= Self.pinnedAnswers }

    /// Yes, No and two "always" answers: the most an approval offers.
    private static let pinnedAnswers = 4

    private var header: some View {
        let tint = Home.AgentStatus.blocked.color
        return HStack(spacing: 12) {
            Image(systemName: "terminal.fill").font(.title3)
                .foregroundStyle(tint)
                .frame(width: 40, height: 40)
                .background(tint.opacity(0.15), in: .rect(cornerRadius: 12))
                .accessibilityHidden(true)
            Text("Run this command?").font(.headline)
            Spacer(minLength: 0)
        }
    }

    /// Two lines until opened; the whole command, selectable, once open.
    private func commandCard(_ command: PermissionCommand) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Button(action: toggle) {
                HStack(alignment: expanded ? .top : .center, spacing: 8) {
                    Text(command.text)
                        .font(.system(expanded ? .callout : .footnote, design: .monospaced))
                        .lineLimit(expanded ? nil : 2)
                        .multilineTextAlignment(.leading)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    Image(systemName: expanded ? "arrow.down.right.and.arrow.up.left" : "arrow.up.left.and.arrow.down.right")
                        .foregroundStyle(.secondary)
                }
                .padding(14)
                .background(Color.primary.opacity(0.06), in: .rect(cornerRadius: 16))
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(command.text)
            .accessibilityHint(expanded ? "Shows less" : "Shows the whole command")
            HStack {
                if command.moreLines > 0 {
                    Text(command.moreLines == 1 ? "+1 more line" : "+\(command.moreLines) more lines")
                }
                Spacer(minLength: 0)
                if expanded {
                    Button("Copy", systemImage: "doc.on.doc") {
                        UIPasteboard.general.string = command.text
                        Feedback.shared.touch(.select)
                    }
                }
            }
            .font(.caption).foregroundStyle(.secondary)
        }
    }

    /// Any other request: its title, shortened until tapped.
    private var titleText: some View {
        Text(permission.title)
            .font(.title3.weight(.semibold))
            .lineLimit(expanded ? nil : 6)
            .fixedSize(horizontal: false, vertical: true)
            .textSelection(.enabled)
            .onTapGesture(perform: toggle)
    }

    private func toggle() {
        withAnimation(.snappy) {
            expanded.toggle()
            detent = expanded ? .large : .medium
        }
    }

    /// Two answers side by side, refusing first; more stack. The "always"
    /// answers sit below, set apart.
    private var answers: some View {
        let always = permission.options.filter { $0.kind == .allowAlways || $0.kind == .rejectAlways }
        let rest = permission.options.filter { !always.contains($0) }
        let once = rest.filter { $0.kind != .allowOnce } + rest.filter { $0.kind == .allowOnce }
        return VStack(spacing: 10) {
            if once.count == 2 {
                HStack(spacing: 12) { ForEach(once) { button($0) } }
            } else {
                ForEach(once) { button($0) }
            }
            ForEach(always) { option in
                Button(option.name) { answer(option) }.font(.subheadline)
            }
        }
        .padding(.horizontal, 24).padding(.top, 10).padding(.bottom, 8)
    }

    @ViewBuilder
    private func button(_ option: PermissionOption) -> some View {
        if option.kind == .allowOnce {
            Button { answer(option) } label: { Text(option.name).frame(maxWidth: .infinity) }
                .buttonStyle(.glassProminent).controlSize(.large).tint(Home.AgentStatus.blocked.color)
        } else {
            Button { answer(option) } label: { Text(option.name).frame(maxWidth: .infinity) }
                .buttonStyle(.glass).controlSize(.large)
        }
    }
}
