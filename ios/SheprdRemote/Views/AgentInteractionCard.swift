import SheprdRemoteCore
import SwiftUI

extension EnvironmentValues {
    /// Sends a user reply back to the open chat (returns `nil` on success or
    /// an error message when delivery fails). `nil` in read-only previews.
    @Entry var sendChatReply: (@MainActor (String) async -> String?)?
}

/// Native SwiftUI presentation for persistent agent interactions (`:::modal`,
/// `:::urgent`, `:::countdown`, `:::otp`, `:::oauth`, `:::secret`, `:::progress`,
/// `:::runbook`, `:::drop`, `:::diff`, `:::spotlight`) inside iOS/iPadOS chats.
/// Respects `LockState.shared.hidesNames` so sensitive titles/secrets stay hidden
/// behind Face ID App Lock.
struct AgentInteractionCard: View {
    let interaction: AgentInteraction
    var onReply: (@MainActor (String) async -> String?)?

    @State private var completedSteps: Set<Int> = []
    @State private var feedback: String?
    @State private var errorMessage: String?
    @State private var sending = false
    @State private var dismissed = false
    @Environment(\.openURL) private var openURL
    @Environment(\.sendChatReply) private var envSendReply

    private var replySender: (@MainActor (String) async -> String?)? {
        onReply ?? envSendReply
    }

    var body: some View {
        if dismissed {
            Button {
                dismissed = false
            } label: {
                Label("Show \(interaction.title)", systemImage: iconName)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
        } else if LockState.shared.hidesNames {
            HStack(spacing: 10) {
                Image(systemName: "lock.fill")
                    .foregroundStyle(.secondary)
                Text("Unlock Sheprd to view agent request")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .glassEffect(in: .rect(cornerRadius: 16))
        } else {
            cardContent
                .padding(14)
                .frame(maxWidth: .infinity, alignment: .leading)
                .glassEffect(in: .rect(cornerRadius: 18))
                .overlay(
                    RoundedRectangle(cornerRadius: 18)
                        .stroke(accentColor.opacity(0.65), lineWidth: interaction.scope == .all ? 1.5 : 1)
                )
        }
    }

    private func send(_ text: String, confirmation: String, onSuccess: (@MainActor () -> Void)? = nil) {
        guard let sender = replySender, !sending else { return }
        sending = true
        errorMessage = nil
        Task {
            let problem = await sender(text)
            sending = false
            if let problem {
                errorMessage = problem
            } else {
                onSuccess?()
                feedback = confirmation
                Feedback.shared.touch(.send)
            }
        }
    }

    private var iconName: String {
        switch interaction.directive {
        case .urgent: "key.fill"
        case .modal: "bell.badge.fill"
        case .countdown: "timer"
        case .otp: "number.square.fill"
        case .oauth: "arrow.up.right.square.fill"
        case .secret: "lock.shield.fill"
        case .progress: "chart.bar.fill"
        case .runbook: "checklist"
        case .drop: "square.and.arrow.up.fill"
        case .diff: "doc.text.magnifyingglass"
        case .spotlight: "sparkles"
        }
    }

    private var accentColor: Color {
        switch interaction.directive {
        case .urgent, .otp, .oauth, .countdown: .orange
        case .secret: .purple
        case .progress:
            if case .progress(_, _, true) = interaction.payload { .green } else { .accentColor }
        default: .accentColor
        }
    }

    @ViewBuilder
    private var cardContent: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                Image(systemName: iconName)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(accentColor)
                Text(interaction.scope == .all ? "URGENT" : interaction.directive.rawValue.uppercased())
                    .font(.caption2.weight(.bold))
                    .foregroundStyle(accentColor)
                Spacer()
                Button {
                    dismissed = true
                } label: {
                    Image(systemName: "xmark")
                        .font(.caption2.weight(.bold))
                        .foregroundStyle(.secondary)
                        .frame(width: 24, height: 24)
                        .background(Circle().fill(.quaternary))
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Dismiss")
            }

            Text(interaction.title)
                .font(.headline)
                .foregroundStyle(.primary)

            if !interaction.body.isEmpty {
                Text(interaction.body)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }

            payloadView

            if let errorMessage {
                Text(errorMessage)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.red)
            } else if let feedback {
                Text(feedback)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.green)
            }
        }
    }

    @ViewBuilder
    private var payloadView: some View {
        switch interaction.payload {
        case let .modal(_, choices, _):
            if !choices.isEmpty {
                HStack(spacing: 8) {
                    ForEach(choices, id: \.self) { choice in
                        Button(choice) {
                            send(choice, confirmation: "Sent: \(choice)")
                        }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.small)
                        .disabled(replySender == nil || sending)
                    }
                }
            }
        case let .countdown(seconds):
            HStack {
                Text("Proceeds in \(seconds)s")
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.orange)
                Spacer()
                Button("Cancel", role: .destructive) {
                    send("Cancel the scheduled action.", confirmation: "Cancelled")
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
                .disabled(replySender == nil || sending)
            }
        case .otp:
            Label(
                "Enter your one-time code directly in Sheprd on your Mac so it is never stored in chat transcripts.",
                systemImage: "desktopcomputer"
            )
            .font(.caption)
            .foregroundStyle(.secondary)
        case let .oauth(url, code):
            VStack(alignment: .leading, spacing: 6) {
                if let code {
                    Text("Code: \(code)")
                        .font(.subheadline.monospaced().weight(.semibold))
                }
                if let parsed = URL(string: url), ["https", "http"].contains(parsed.scheme?.lowercased()) {
                    Button("Open sign-in page") {
                        openURL(parsed)
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
                }
            }
        case let .secret(key, dest, _):
            VStack(alignment: .leading, spacing: 6) {
                Text("Key: \(key) → \(dest)")
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                Label(
                    "Secret injection is handled on your Mac in Sheprd so credentials never enter chat transcripts or cross the relay.",
                    systemImage: "lock.shield"
                )
                .font(.caption)
                .foregroundStyle(.secondary)
            }
        case let .progress(percent, step, done):
            VStack(alignment: .leading, spacing: 6) {
                if !step.isEmpty {
                    Text(step)
                        .font(.caption.monospaced())
                        .foregroundStyle(.secondary)
                }
                if let percent {
                    ProgressView(value: Double(min(100, max(0, percent))), total: 100)
                        .tint(done ? .green : .accentColor)
                }
            }
        case let .runbook(steps):
            VStack(alignment: .leading, spacing: 6) {
                ForEach(Array(steps.enumerated()), id: \.offset) { idx, step in
                    let checked = step.done || completedSteps.contains(idx)
                    Button {
                        guard step.human, !checked else { return }
                        send(
                            "Completed runbook step: \(step.label)",
                            confirmation: "Completed: \(step.label)"
                        ) {
                            completedSteps.insert(idx)
                        }
                    } label: {
                        HStack(spacing: 8) {
                            Image(systemName: checked ? "checkmark.circle.fill" : "circle")
                                .foregroundStyle(checked ? .green : (step.human ? .orange : .secondary))
                            Text(step.label)
                                .font(.subheadline)
                                .foregroundStyle(.primary)
                            Spacer()
                            Text(step.human ? "You" : "Agent")
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                    }
                    .buttonStyle(.plain)
                    .disabled(!step.human || checked || replySender == nil || sending)
                }
            }
        case let .drop(accept):
            Text(accept.isEmpty ? "Use the attachment button below to send a photo or file." : "Requested attachment (\(accept)) — use the attachment button below.")
                .font(.caption)
                .foregroundStyle(.secondary)
        case let .diff(file, diff):
            VStack(alignment: .leading, spacing: 8) {
                if !file.isEmpty {
                    Text(file).font(.caption.monospaced()).foregroundStyle(.secondary)
                }
                ScrollView(.horizontal) {
                    Text(diff)
                        .font(.caption.monospaced())
                        .padding(8)
                        .background(Color.black.opacity(0.25), in: RoundedRectangle(cornerRadius: 8))
                }
                HStack(spacing: 8) {
                    Button("Reject", role: .destructive) {
                        send("Rejected proposed diff.", confirmation: "Rejected")
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .disabled(replySender == nil || sending)
                    Button("Approve") {
                        send("Approved proposed diff — proceed.", confirmation: "Approved")
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
                    .disabled(replySender == nil || sending)
                }
            }
        case let .spotlight(target, step):
            Text("Target: \(target)\(step.map { " · step \($0)" } ?? "")")
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
        }
    }
}
