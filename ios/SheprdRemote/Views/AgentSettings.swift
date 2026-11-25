import SheprdRemoteCore
import SwiftUI

/// Destinations used by both ordinary links and an agent's navigation.
struct AgentSettingsDestination: View {
    let page: AgentSettingsPage

    var body: some View {
        switch page {
        case .chats: ChatSettingsView()
        case .computers: ComputersSettingsView()
        case .delivery: PushDeliveryView()
        case .deskHold: DeskHoldView()
        case .fleet: FleetSettingsView()
        case .github: GitHubSettingsView()
        case .notifications: NotificationSettingsView()
        case .privacy: PrivacySettingsView()
        case .relays: RelaysView()
        case .sounds: SoundsSettingsView()
        case .terminalKeys: TerminalKeysView()
        case .textShortcuts: TextShortcutsView()
        case .voice: VoiceSettingsView()
        }
    }
}

extension View {
    func agentHighlight(_ target: AgentHighlight) -> some View {
        modifier(AgentHighlightModifier(target: target))
    }

    func agentGuideScrolling() -> some View {
        AgentGuideScrolling(content: self)
    }
}

private struct AgentHighlightModifier: ViewModifier {
    @Environment(AppModel.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.colorScheme) private var colorScheme
    let target: AgentHighlight

    func body(content: Content) -> some View {
        content
            .id(target)
            .overlay {
                if model.agents.highlight == target, !LockState.shared.hidesNames {
                    RoundedRectangle(cornerRadius: 13, style: .continuous)
                        .fill(Color.orange.opacity(colorScheme == .dark ? 0.24 : 0.16))
                        .padding(.horizontal, -9)
                        .padding(.vertical, -8)
                        .allowsHitTesting(false)
                        .accessibilityHidden(true)
                }
            }
            .animation(reduceMotion ? nil : .easeInOut(duration: 0.2), value: model.agents.highlight)
    }
}

private struct AgentGuideScrolling<Content: View>: View {
    @Environment(AppModel.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    let content: Content

    var body: some View {
        ScrollViewReader { proxy in
            content.task(id: model.agents.guide?.id) {
                guard let target = model.agents.highlight else { return }
                // Let a new destination lay out its list before scrolling.
                try? await Task.sleep(for: .milliseconds(100))
                guard !Task.isCancelled else { return }
                withAnimation(reduceMotion ? nil : .easeInOut(duration: 0.2)) {
                    proxy.scrollTo(target, anchor: .center)
                }
            }
        }
    }
}
