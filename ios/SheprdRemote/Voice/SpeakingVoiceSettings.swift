import CryptoKit
import SwiftUI

struct SpeakingVoiceSettings: View {
    @State private var saved: SpeechAccount?
    @State private var provider: SpeechProvider?
    @State private var showingProviders = false
    @State private var editing = false
    @State private var detent: PresentationDetent = .height(500)
    @Environment(\.dynamicTypeSize) private var typeSize
    @State private var key = ""
    @State private var voice = ""
    @State private var choices: [SpeechAccount.Choice] = []
    @State private var message = ""
    @State private var previewing = false
    @State private var busy = false
    @State private var speaker = VoiceSpeaker()
    @State private var verifier: String?
    @State private var code = ""
    @AppStorage("voiceKeyboardInput") private var keyboard = false

    private var account: SpeechAccount? {
        provider.map { SpeechAccount(provider: $0, voice: voice, key: key.trimmingCharacters(in: .whitespacesAndNewlines)) }
    }

    var body: some View {
        Section("Speaking Voice") {
            Button {
                editing = false
                detent = typeSize.isAccessibilitySize ? .large : .height(500)
                showingProviders = true
            } label: {
                HStack(spacing: 12) {
                    providerLabel(saved?.provider)
                    Spacer()
                    Text("Change").foregroundStyle(Color.accentColor)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("speech-provider")
            .sheet(isPresented: $showingProviders, onDismiss: {
                speaker.stop()
                reset()
            }) {
                NavigationStack {
                    VStack(spacing: 0) {
                        HStack {
                            Text("Choose a provider").font(.title2.bold())
                                .accessibilityAddTraits(.isHeader)
                            Spacer()
                            Button("Close", systemImage: "xmark") { showingProviders = false }
                                .labelStyle(.iconOnly)
                                .buttonStyle(.bordered)
                                .buttonBorderShape(.circle)
                                .tint(.secondary)
                                .accessibilityIdentifier("close-provider-list")
                        }
                        .padding(.horizontal, 20)
                        .padding(.top, 24)
                        List {
                            Section {
                                providerRow(nil)
                                ForEach(SpeechProvider.allCases, id: \.self) { providerRow($0) }
                            } footer: {
                                Text("Choose a cloud provider to connect your account.")
                            }
                        }
                        .contentMargins(.top, 12, for: .scrollContent)
                    }
                    .background(Color(uiColor: .systemGroupedBackground))
                    .toolbar(.hidden, for: .navigationBar)
                    .navigationDestination(isPresented: $editing) {
                        providerSettings
                    }
                }
                .presentationDetents([.height(500), .large], selection: $detent)
                .presentationBackground(Color(uiColor: .systemGroupedBackground))
                .presentationDragIndicator(.visible)
                .interactiveDismissDisabled(busy)
            }
            if !showingProviders && !message.isEmpty { Text(message).font(.footnote) }
        }
        .onAppear {
            do { saved = try SpeechAccount.load() }
            catch { message = error.localizedDescription }
        }

        Section {
            Toggle("Use Keyboard for Voice Commands", isOn: $keyboard)
        } header: {
            Text("Dictation")
        } footer: {
            Text("For Wispr Flow, enable its keyboard in iOS Settings, then select it with the globe key.")
        }
    }

    private func providerLabel(_ option: SpeechProvider?) -> some View {
        HStack(spacing: 12) {
            ProviderMark(provider: option)
            VStack(alignment: .leading, spacing: 2) {
                Text(option?.name ?? "Apple").font(.body.weight(.semibold))
                Text(option == nil ? "On device" : option == .openrouter ? "Sign-in or API key" : "API key")
                    .font(.subheadline).foregroundStyle(.secondary)
            }
        }
    }

    private func providerRow(_ option: SpeechProvider?) -> some View {
        Button {
            provider = option
            reset()
            restore(matching: option)
            detent = .large
            editing = true
        } label: {
            HStack(spacing: 8) {
                providerLabel(option)
                Spacer()
                if option == saved?.provider {
                    Label("In use", systemImage: "checkmark.circle.fill")
                        .font(.caption.weight(.semibold)).foregroundStyle(.green)
                } else {
                    Image(systemName: "chevron.right").font(.caption.weight(.semibold))
                        .foregroundStyle(.tertiary)
                }
            }
            .padding(.vertical, 3)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("voice-option-\(option?.rawValue ?? "apple")")
    }

    private var providerSettings: some View {
        Form {
            Section {
                if let provider {
                    SecureField("API key", text: $key)
                        .textInputAutocapitalization(.never).autocorrectionDisabled()
                    if provider == .openrouter {
                        Button("Sign in with OpenRouter") { beginSignIn() }
                        if verifier != nil {
                            SecureField("Authorization code from OpenRouter", text: $code)
                            Button("Finish Sign In") { run { try await finishSignIn() } }
                                .disabled(code.isEmpty)
                        }
                    }
                    if provider == .elevenlabs {
                        Button("Load My Voices") { run { choices = try await account?.choices() ?? [] } }
                            .disabled(account?.valid != true)
                    }
                    Menu("Choose Voice") {
                        ForEach(provider.voices, id: \.self) { id in Button(id) { voice = id } }
                        ForEach(choices) { choice in Button(choice.name) { voice = choice.id } }
                    }
                    TextField("Voice ID", text: $voice).textInputAutocapitalization(.never).autocorrectionDisabled()
                }
                Button("Preview") {
                    let selected = account
                    previewing = true
                    run {
                        defer { previewing = false }
                        guard try await speaker.play("Two chats need you. Would you like to open the first?", account: selected)
                        else { throw SpeechFailure.playback }
                    }
                }
                Button(provider == nil ? "Use Apple Voice" : "Use This Voice") {
                    do {
                        try SpeechAccount.save(account)
                        saved = account
                        showingProviders = false
                        message = "Using \(provider?.name ?? "Apple")"
                    } catch { message = error.localizedDescription }
                }
                .disabled(account == saved || (provider != nil && account?.valid != true))
                if let savedProvider = saved?.provider, savedProvider == provider {
                    Button("Disconnect", role: .destructive) {
                        do {
                            try SpeechAccount.save(nil)
                            saved = nil
                            showingProviders = false
                            provider = nil
                            reset()
                        } catch { message = error.localizedDescription }
                    }
                }
                if busy { ProgressView() }
                if !message.isEmpty { Text(message).font(.footnote) }
            } footer: {
                Text("Cloud voices are AI-generated. Spoken text goes to the selected provider and uses your credits. Pairing copies the active voice and API key to a device without a voice account.")
            }
        }
        .toolbar(.visible, for: .navigationBar)
        .navigationTitle(provider?.name ?? "Apple")
        .navigationBarTitleDisplayMode(.inline)
        .navigationBarBackButtonHidden(busy)
        .disabled(busy)
        .safeAreaInset(edge: .bottom) {
            if previewing {
                Button("Stop preview", systemImage: "stop.fill") { speaker.stop() }
                    .buttonStyle(.borderedProminent)
                    .padding(.bottom, 8)
            }
        }
    }

    private func reset() {
        key = ""
        voice = provider?.voices.first ?? ""
        verifier = nil
        code = ""
        choices = []
        message = ""
    }

    private func restore(matching provider: SpeechProvider?) {
        if let saved, saved.provider == provider {
            key = saved.key
            voice = saved.voice
        }
    }

    private func run(_ operation: @escaping @MainActor () async throws -> Void) {
        busy = true
        message = ""
        Task {
            defer { busy = false }
            do { try await operation() } catch { message = error.localizedDescription }
        }
    }

    private func beginSignIn() {
        var random = [UInt8](repeating: 0, count: 32)
        guard SecRandomCopyBytes(kSecRandomDefault, random.count, &random) == errSecSuccess else { return }
        let secret = Data(random).base64URLEncoded
        let challenge = Data(SHA256.hash(data: Data(secret.utf8))).base64URLEncoded
        guard let url = URL(string: "https://openrouter.ai/auth?code_challenge_method=S256&key_label=Sheprd&code_challenge=\(challenge)") else { return }
        verifier = secret
        code = ""
        // Always the system browser: this runs in the provider sheet, which
        // the app's own link browsing, further up, cannot open over.
        UIApplication.shared.open(url)
    }

    private func finishSignIn() async throws {
        struct Response: Decodable { let key: String }
        guard let verifier, let url = URL(string: "https://openrouter.ai/api/v1/auth/keys") else { throw SpeechFailure.invalid }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONEncoder().encode(["code": code.trimmingCharacters(in: .whitespacesAndNewlines), "code_verifier": verifier, "code_challenge_method": "S256"])
        key = try await JSONDecoder().decode(Response.self, from: SpeechHTTP.fetch(request, limit: 32_768)).key
        self.verifier = nil
        code = ""
        message = "Connected. Preview the voice, then select Use This Voice."
    }
}

private extension Data {
    var base64URLEncoded: String {
        base64EncodedString().replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
    }
}

private struct ProviderMark: View {
    let provider: SpeechProvider?

    private var tint: Color {
        switch provider {
        case nil, .elevenlabs: .primary
        case .openai: .teal
        case .deepgram: .green
        case .openrouter: .indigo
        }
    }

    var body: some View {
        Group {
            if let provider {
                Image(provider == .openai ? "Agents/codex" : "VoiceProviders/\(provider.rawValue)")
                    .renderingMode(.template).resizable().scaledToFit()
            } else {
                Image(systemName: "apple.logo").resizable().scaledToFit()
            }
        }
        .frame(width: 23, height: 23)
        .frame(width: 40, height: 40)
        .foregroundStyle(tint)
        .background(tint.opacity(0.09), in: RoundedRectangle(cornerRadius: 10))
        .accessibilityHidden(true)
    }
}
