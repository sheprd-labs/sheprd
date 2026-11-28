import SheprdRemoteCore
import SwiftUI
import UIKit

/// Settings > Chats.
struct ChatSettingsView: View {
    @Environment(AppModel.self) private var model
    @AppStorage(RowLines.key) private var rowLines = RowLines.detailDefault
    @AppStorage(RowLines.titleKey) private var titleLines = RowLines.titleDefault

    var body: some View {
        List {
            Section {
                Picker("Title Lines", selection: $titleLines) {
                    ForEach(RowLines.choices, id: \.self) { Text($0, format: .number).tag($0) }
                }
                Picker("Lines per Row", selection: $rowLines) {
                    ForEach(RowLines.choices, id: \.self) { Text($0, format: .number).tag($0) }
                }
                .agentHighlight(.chatRows)
            } footer: {
                Text("How much of a chat's title, and of its latest reply or summary, its row shows in the lists.")
            }
            Section {
                NavigationLink(value: AgentSettingsPage.textShortcuts) {
                    LabeledContent("Text Shortcuts", value: "\(model.shortcuts.shortcuts.count)")
                }
            }
        }
        .agentGuideScrolling()
        .navigationTitle("Chats")
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// Settings > Voice.
struct VoiceSettingsView: View {
    @AppStorage(VoiceSettings.speaksKey) private var voiceSpeaks = true
    @AppStorage(VoiceSettings.confirmsKey) private var voiceConfirms = false
    @AppStorage(VoiceSettings.languageKey) private var voicePhoneLanguage = false

    var body: some View {
        List {
            SpeakingVoiceSettings()
            Section {
                Toggle("Speak Results", isOn: $voiceSpeaks).agentHighlight(.speakResults)
            }
            Section {
                Toggle("Confirm by Voice", isOn: $voiceConfirms).agentHighlight(.voiceConfirmation)
            } footer: {
                Text("Reads the plan aloud and starts on a plain yes, for a new worktree only. Anything else still needs a tap.")
            }
            Section {
                Toggle("Use Phone Language", isOn: $voicePhoneLanguage).agentHighlight(.voiceLanguage)
            } footer: {
                Text("Experimental: commands other than English are less reliable, and cannot stop or answer a chat.")
            }
        }
        .agentGuideScrolling()
        .navigationTitle("Voice")
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// Settings > Sounds & Haptics.
struct SoundsSettingsView: View {
    @AppStorage(FeedbackSettings.soundsKey) private var effectSounds = true
    @AppStorage(FeedbackSettings.hapticsKey) private var haptics = true

    var body: some View {
        List {
            Section {
                Toggle("Effect Sounds", isOn: $effectSounds).agentHighlight(.sounds)
            } footer: {
                Text(FeedbackSettings.canTap
                    ? "Sounds that go with an agent's message effects, such as slam and fireworks. Quiet when the Ring/Silent switch is on silent."
                    : "Sounds that go with an agent's message effects, such as slam and fireworks. Quiet when the \(UIDevice.current.model) is muted.")
            }
            if FeedbackSettings.canTap {
                Section {
                    Toggle("Haptics", isOn: $haptics).agentHighlight(.haptics)
                } footer: {
                    Text("What you feel on sending, answering, terminal keys, and effects.")
                }
            }
        }
        .agentGuideScrolling()
        .navigationTitle("Sounds & Haptics")
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// Settings > Privacy & Storage: the Face ID lock and the cache.
struct PrivacySettingsView: View {
    @Environment(AppModel.self) private var model
    @AppStorage(AppLock.setting, store: AppLock.store) private var requireFaceID = false
    @State private var cacheSize = 0

    var body: some View {
        List {
            Section {
                Toggle("Require Face ID", isOn: Binding(
                    get: { requireFaceID },
                    set: { on in
                        // Turning the lock off asks too, so a borrowed,
                        // unlocked phone cannot drop it.
                        Task {
                            if await OwnerCheck.confirm(on ? "Lock Sheprd" : "Stop locking Sheprd") {
                                requireFaceID = on
                                model.appLockChanged()
                            }
                        }
                    }
                ))
                .agentHighlight(.faceId)
            } footer: {
                Text("Asks for Face ID when the app opens and hides it in the app switcher. \"Always\" answers ask either way.")
            }
            Section("Storage") {
                LabeledContent("Cached chats and lists", value: ByteCountFormatter.string(fromByteCount: Int64(cacheSize), countStyle: .file)).agentHighlight(.storage)
                Button("Clear Cache", role: .destructive) {
                    model.fleet.clearCache()
                    model.drafts = [:]
                    cacheSize = 0
                }
            }
        }
        .agentGuideScrolling()
        .navigationTitle("Privacy & Storage")
        .navigationBarTitleDisplayMode(.inline)
        .task { cacheSize = model.fleet.cache.size() }
    }
}
