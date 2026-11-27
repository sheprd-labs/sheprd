import SwiftUI
import UIKit

/// A card along the top saying what an agent on a computer showed or
/// opened (`AgentControl`): its note, or that it opened the screen now in
/// front, so the screen never changes unexplained. A note goes by itself; a
/// guide stays, on a bar of its own above the app. A tap opens the asking
/// agent's chat when it named one, and the close button only dismisses.
struct AgentBanner: View {
    @Environment(AppModel.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        // Nothing shows behind the lock, whose cover is above this window.
        let banner = LockState.shared.hidesNames ? nil : (model.agents.guide ?? model.agents.banner)
        let guiding = model.agents.guide != nil
        Group {
            if let banner {
                content(banner)
                    .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { height in
                        if model.agents.guide != nil { model.agents.guideHeight = height }
                    }
                    // A guide's bar, which the app is pushed below, reads
                    // as part of the screen rather than a gap above it.
                    .background(alignment: .bottom) {
                        if guiding {
                            Rectangle().fill(.bar).ignoresSafeArea(edges: .top)
                                .overlay(alignment: .bottom) { Divider() }
                        }
                    }
                    .transition(.move(edge: .top).combined(with: .opacity))
            }
        }
        .animation(reduceMotion ? nil : .spring(duration: 0.35), value: banner)
        .task(id: banner?.id) {
            guard let banner, model.agents.guide == nil else { return }
            try? await Task.sleep(for: banner.shown)
            if !Task.isCancelled, model.agents.banner?.id == banner.id { model.agents.banner = nil }
        }
    }

    private func content(_ banner: AgentControl.Banner) -> some View {
        let opens = banner.from != nil
        return HStack(alignment: .center, spacing: 12) {
            Image(systemName: banner.note ? "sparkles" : "arrow.up.forward.app")
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(.white)
                .frame(width: 34, height: 34)
                .background(Circle().fill(Color.accentColor.gradient))
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                if banner.note {
                    HStack(spacing: 4) {
                        Text(banner.computer).fontWeight(.semibold).foregroundStyle(.primary)
                        Text("· Agent").foregroundStyle(.secondary)
                        if opens {
                            Spacer(minLength: 4)
                            Text("Open chat").foregroundStyle(.tint)
                            Image(systemName: "chevron.right")
                                .font(.caption2.weight(.bold))
                                .foregroundStyle(.tint)
                        }
                    }
                    .font(.caption)
                    .lineLimit(1)
                }
                Text(banner.text)
                    .font(.subheadline)
                    .foregroundStyle(.primary)
                    .lineLimit(banner.note ? 2 : 1)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            Button {
                model.agents.banner = nil
                model.agents.clearGuide()
            } label: {
                Image(systemName: "xmark")
                    .font(.system(size: 11, weight: .bold))
                    .foregroundStyle(.secondary)
                    .frame(width: 26, height: 26)
                    .background(Circle().fill(.quaternary))
                    .contentShape(.circle)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Dismiss")
        }
        .padding(.leading, 12)
        .padding(.trailing, 10)
        .padding(.vertical, 10)
        .glassEffect(in: .rect(cornerRadius: 22))
        .padding(.horizontal, 12)
        .padding(.top, 4)
        .padding(.bottom, model.agents.guide == nil ? 0 : 8)
        .contentShape(.rect)
        .onTapGesture { model.agents.tapped(banner, model: model) }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isButton)
        .accessibilityHint(opens ? "Opens the agent's chat" : "Dismisses it")
        .accessibilityAction(named: "Dismiss") {
            model.agents.banner = nil
            model.agents.clearGuide()
        }
    }
}

/// A window over each scene for the banner, so it shows above sheets such
/// as the new workspace form an agent opened. It sits below the app lock's
/// cover (`.alert + 1`) and passes every touch but the banner's through.
@MainActor
enum AgentBannerWindows {
    private static var windows: [ObjectIdentifier: UIWindow] = [:]

    /// Puts a banner window over every connected scene that has none.
    static func install(model: AppModel) {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        for (id, window) in windows where window.windowScene == nil || !scenes.contains(where: { ObjectIdentifier($0) == id }) {
            window.isHidden = true
            windows[id] = nil
        }
        for scene in scenes where windows[ObjectIdentifier(scene)]?.windowScene !== scene {
            let window = PassThroughWindow(windowScene: scene)
            window.windowLevel = .alert
            let host = UIHostingController(
                // Along the top, as an overlay would put it; the empty
                // rest takes no touches, so they fall through.
                rootView: AgentBanner()
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                    .environment(model)
            )
            host.view.backgroundColor = .clear
            window.rootViewController = host
            window.isHidden = false
            windows[ObjectIdentifier(scene)] = window
        }
    }
}

/// A window that takes only the touches its content takes: anywhere else
/// falls through to the app below.
private final class PassThroughWindow: UIWindow {
    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        let hit = super.hitTest(point, with: event)
        return hit === rootViewController?.view ? nil : hit
    }
}
