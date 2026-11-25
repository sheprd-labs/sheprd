import SwiftUI
import UIKit

/// A capsule along the top saying what an agent on a computer showed or
/// opened (`AgentControl`): its note, or that it opened the screen now in
/// front, so the screen never changes unexplained. It goes by itself; a
/// tap sends it sooner. It never takes input beyond that tap.
struct AgentBanner: View {
    @Environment(AppModel.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        // Nothing shows behind the lock, whose cover is above this window.
        let banner = LockState.shared.hidesNames ? nil : (model.agents.guide ?? model.agents.banner)
        Group {
            if let banner {
                content(banner)
                    .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { height in
                        if model.agents.guide != nil { model.agents.guideHeight = height }
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
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Image(systemName: banner.note ? "text.bubble" : "sparkles")
                .foregroundStyle(.tint)
            VStack(alignment: .leading, spacing: 2) {
                if banner.note {
                    Text("From \(banner.computer)'s agent")
                        .font(.caption.weight(.semibold))
                        .lineLimit(1)
                        .foregroundStyle(.secondary)
                }
                Text(banner.text)
                    .font(.subheadline)
                    .lineLimit(banner.note ? 2 : 1)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .glassEffect(in: .rect(cornerRadius: 18))
        .padding(.horizontal, 16)
        .padding(.top, 4)
        .contentShape(.rect)
        .onTapGesture { model.agents.banner = nil; model.agents.clearGuide() }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isButton)
        .accessibilityHint("Dismisses it")
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
