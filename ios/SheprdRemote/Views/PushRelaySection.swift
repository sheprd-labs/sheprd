import SheprdRemoteCore
import SwiftUI

/// Settings' push relay section: the opt-in, off by default, with what the
/// push relay sees (`docs/apns-relay-plan.md`, Opt-in), and how each paired
/// computer delivers this phone's pushes. Turning it on asks first, with
/// the same explanation, as Sheprd does on the computer.
struct PushRelaySection: View {
    @Environment(AppModel.self) private var model
    @AppStorage(PushRelaySetting.key, store: PushRelaySetting.store) private var pushRelay = false
    @State private var confirming = false

    /// The plan's explanation, kept true as the design changes.
    static let explanation = """
    Get Live Activities and alerts when you are away from your computers. Apple only accepts pushes for this app from Sheprd, so they go through a small server Sheprd runs.

    What the push relay sees: this phone's push token while it sends a push, the time and size of each push, and the IP addresses of your computer and phone.

    What it never sees: your prompts, chats, code, file names, or terminal output. Alerts and Live Activities are encrypted so that only this phone can read them.

    What it cannot do: connect to your computer, run anything, or answer for you. Tokens are not stored or logged.

    Apple also sees the push token, timing, and size of every push. The computer must have the push relay on too, in Sheprd's Remote control settings.
    """

    var body: some View {
        Section {
            Toggle("Use the Push Relay", isOn: Binding(
                get: { pushRelay },
                set: { on in
                    if on {
                        confirming = true
                    } else {
                        pushRelay = false
                        Task { await model.tokens.pushRelayChanged(to: false) }
                    }
                }
            ))
            ForEach(model.computers) { computer in
                LabeledContent(model.displayName(computer), value: Self.label(model.tokens.delivery[computer.fingerprint], pushRelay: pushRelay))
            }
            if pushRelay, let error = model.tokens.pushRelay.lastError {
                Text(error).font(.footnote).foregroundStyle(.secondary)
            }
            if !pushRelay, model.tokens.delivery.values.contains(.pushRelay) {
                Text("A computer sends pushes through the push relay. Turn it on here to get its alerts and Live Activities.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
        } header: {
            Text("Push Relay")
        } footer: {
            Text(pushRelay
                ? "On. The computers get sealed handles instead of this phone's push tokens. Turning it off revokes them."
                : "Off. Pushes reach this phone only from a computer with its own APNs key, for a copy of the app you sign yourself.")
        }
        .alert("Push through the Sheprd push relay?", isPresented: $confirming) {
            Button("Turn On") {
                pushRelay = true
                Task { await model.tokens.pushRelayChanged(to: true) }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(Self.explanation)
        }
    }

    /// How one computer delivers, as far as its last answer said.
    static func label(_ delivery: ActivityTokens.Delivery?, pushRelay: Bool) -> String {
        switch delivery {
        case .pushRelay: pushRelay ? "Push relay" : "Push relay, off here"
        case .ownKey: pushRelay ? "Own APNs key, which needs the push relay off" : "Own APNs key"
        case .off: "No push set up"
        case .unknown: "Not reported"
        case nil: "Not asked yet"
        }
    }
}
