import SheprdRemoteCore
import PhotosUI
import SwiftUI
import UIKit
import UniformTypeIdentifiers

/// The chat's composer: text with `/` completion, pictures from the
/// library, the camera, or the clipboard, text shortcuts, dictation on
/// the phone, and send or stop.
struct Composer: View {
    /// Where what is composed goes.
    enum Destination {
        /// The chat's program, as a prompt.
        case chat
        /// A terminal agent's own terminal, whose conversation is shown
        /// read only: pictures as dropped files, then the words, then
        /// Enter; stopping presses Esc.
        case terminal
    }

    @Environment(AppModel.self) private var model
    @Environment(\.scenePhase) private var scenePhase
    let key: PaneKey
    let store: ChatStore
    let transcript: Transcript
    var destination = Destination.chat
    @Binding var problem: String?
    @State private var draft = ""
    /// Tells this composer from another window's on the same chat.
    @State private var id = UUID()
    @State private var pictures: [Picture] = []
    @State private var picks: [PhotosPickerItem] = []
    @State private var choosingPhotos = false
    @State private var takingPhoto = false
    @State private var clipboardHasImage = UIPasteboard.general.hasImages
    @State private var dictation = Dictation()
    /// The draft before dictation began, which dictated words follow.
    @State private var beforeDictation = ""
    @State private var focused = false
    /// Counts sends, for the haptic.
    @State private var sends = 0
    /// The last message on its way to a terminal: the next waits for it, so
    /// two messages never interleave their pictures, words, and Enter.
    @State private var terminalSend: Task<Void, Never>?

    /// Whether nothing can be sent: a read-only transcript, unless what is
    /// typed goes to the agent's terminal.
    private var readOnly: Bool { transcript.readOnly && destination == .chat }

    var body: some View {
        VStack(spacing: 8) {
            if !completions.isEmpty { completionList }
            // The field itself takes a pasted picture, so the strip shows
            // only what is attached.
            if !pictures.isEmpty { pictureStrip }
            if case let .failed(reason) = dictation.phase {
                Text(reason).font(.footnote).foregroundStyle(.secondary)
            }
            HStack(alignment: .bottom, spacing: 10) {
                attachMenu
                ComposerField(placeholder: "Message \(transcript.agent)", text: $draft, focused: $focused) { images in
                    for image in images.prefix(4 - pictures.count) { add(image) }
                }
                .disabled(readOnly)
                .padding(.horizontal, 16).padding(.vertical, 11)
                    .glassEffect(.regular, in: .rect(cornerRadius: 22))
                trailingButton
            }
        }
        .padding(.horizontal, 16).padding(.bottom, 6)
        .photosPicker(isPresented: $choosingPhotos, selection: $picks, maxSelectionCount: 4, matching: .images)
        .fullScreenCover(isPresented: $takingPhoto) {
            CameraPicker { image in if let image { add(image) } }.ignoresSafeArea()
        }
        .onChange(of: picks) { _, chosen in
            picks = []
            for item in chosen {
                Task {
                    if let data = try? await item.loadTransferable(type: Data.self), let image = UIImage(data: data) {
                        add(image)
                    }
                }
            }
        }
        .onChange(of: dictation.text) { _, words in
            guard dictation.active || !words.isEmpty else { return }
            draft = beforeDictation.isEmpty ? words : beforeDictation + " " + words
        }
        .onReceive(NotificationCenter.default.publisher(for: UIPasteboard.changedNotification)) { _ in
            clipboardHasImage = UIPasteboard.general.hasImages
        }
        .onReceive(NotificationCenter.default.publisher(for: UIApplication.didBecomeActiveNotification)) { _ in
            clipboardHasImage = UIPasteboard.general.hasImages
        }
        .haptic(.start, trigger: dictation.phase == .listening) { _, now in now }
        .haptic(.impact(weight: .light), trigger: sends)
        .onAppear {
            model.fleet.composerOpened(id, in: key)
            // Memory first, which keeps uploads already under way; the
            // saved draft after a relaunch uploads its pictures again.
            if let saved = model.drafts.removeValue(forKey: key) {
                draft = saved.text
                pictures = saved.pictures
            } else if let saved = model.fleet.cache.draft(key.device, pane: key.pane) {
                draft = saved.text
                for jpeg in saved.pictures {
                    if let prepared = PreparedImage(jpeg: jpeg) { add(prepared) }
                }
            }
            #if DEBUG
            // The simulator loop's `-draft TEXT` types into the composer.
            if let text = UserDefaults.standard.string(forKey: "draft") { draft = text }
            #endif
            // Said here too: the camera hides this composer without ending
            // it, and coming back with the same words changes nothing.
            model.fleet.composer(id, in: key, drafted: !draft.isEmpty || !pictures.isEmpty)
        }
        .onDisappear {
            if !draft.isEmpty || !pictures.isEmpty {
                model.drafts[key] = ComposerDraft(text: draft, pictures: pictures)
            }
            saveDraft()
            model.fleet.composerClosed(id, in: key)
        }
        .onChange(of: draft.isEmpty && pictures.isEmpty) { _, empty in
            model.fleet.composer(id, in: key, drafted: !empty)
        }
        // The app may be ended while in the background, with no
        // disappearing first.
        .onChange(of: scenePhase) { _, phase in if phase == .background { saveDraft() } }
    }

    // MARK: Parts

    private var attachMenu: some View {
        Menu {
            Section {
                if clipboardHasImage {
                    Button("Paste Photo", systemImage: "doc.on.clipboard") { pastePhotos() }
                }
                Button("Photo Library", systemImage: "photo.on.rectangle") { choosingPhotos = true }
                if UIImagePickerController.isSourceTypeAvailable(.camera) {
                    Button("Take Photo", systemImage: "camera") { takingPhoto = true }
                }
            }
            .disabled(pictures.count >= 4)
            let shortcuts = model.shortcuts.shortcuts.filter(\.isUsable)
            if !shortcuts.isEmpty {
                Section("Shortcuts") {
                    ForEach(shortcuts) { shortcut in
                        Button(shortcut.label, systemImage: "text.bubble") { use(shortcut) }
                    }
                }
            }
        } label: {
            Image(systemName: "plus").font(.body.bold()).frame(width: 36, height: 36)
        }
        .buttonStyle(.glass)
        .buttonBorderShape(.circle)
        .disabled(readOnly)
        .accessibilityLabel("Add a picture or shortcut")
    }

    @ViewBuilder
    private var trailingButton: some View {
        let stopping = draft.isEmpty && pictures.isEmpty && transcript.status == .working
        if dictation.active {
            Button {
                Task { await dictation.stop() }
            } label: {
                Image(systemName: dictation.phase == .preparing ? "ellipsis" : "stop.circle.fill")
                    .font(.body.bold()).frame(width: 36, height: 36)
                    .symbolEffect(.pulse, isActive: dictation.phase == .listening)
            }
            .buttonStyle(.glassProminent).buttonBorderShape(.circle).tint(.red)
            .accessibilityLabel("Stop dictating")
        } else if draft.isEmpty && pictures.isEmpty && !stopping {
            Button {
                beforeDictation = draft
                Task { await dictation.start() }
            } label: {
                Image(systemName: "mic.fill").font(.body.bold()).frame(width: 36, height: 36)
            }
            .buttonStyle(.glass).buttonBorderShape(.circle)
            .disabled(readOnly)
            .accessibilityLabel("Dictate")
        } else {
            Button {
                if !stopping {
                    sendPrompt()
                    return
                }
                Feedback.shared.touch(.stop)
                if destination == .terminal {
                    toTerminal(keys: ["esc"])
                } else {
                    send(.cancel)
                }
            } label: {
                Image(systemName: stopping ? "stop.fill" : "arrow.up").font(.body.bold()).frame(width: 36, height: 36)
            }
            .buttonStyle(.glassProminent).buttonBorderShape(.circle)
            .disabled(readOnly)
            .accessibilityLabel(stopping ? "Stop" : "Send")
        }
    }

    private var pictureStrip: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(pictures) { picture in
                    Image(uiImage: picture.thumbnail).resizable().scaledToFill()
                        .frame(width: 56, height: 56).clipShape(.rect(cornerRadius: 10))
                        .overlay {
                            if picture.upload == .failed {
                                Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.yellow)
                            }
                        }
                        .overlay(alignment: .bottom) {
                            if picture.upload == .sending {
                                ThinProgress(value: picture.progress.fraction).padding(5)
                            }
                        }
                        .overlay(alignment: .topTrailing) {
                            Button {
                                pictures.removeAll { $0.id == picture.id }
                            } label: {
                                Image(systemName: "xmark.circle.fill").symbolRenderingMode(.palette)
                                    .foregroundStyle(.white, .black.opacity(0.6))
                            }
                            .offset(x: 6, y: -6)
                            .accessibilityLabel("Remove picture")
                        }
                }
            }
            .padding(.top, 6).padding(.horizontal, 2)
        }
    }

    private var completions: [SlashCommand] {
        guard draft.hasPrefix("/"), !draft.contains(where: \.isWhitespace) else { return [] }
        let typed = draft.dropFirst().lowercased()
        return Array(transcript.commands.filter { typed.isEmpty || $0.name.lowercased().hasPrefix(typed) }.prefix(6))
    }

    private var completionList: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(completions, id: \.name) { command in
                Button {
                    draft = "/\(command.name) "
                } label: {
                    VStack(alignment: .leading, spacing: 2) {
                        HStack(spacing: 6) {
                            Text("/\(command.name)").font(.subheadline.monospaced().weight(.semibold))
                            if let hint = command.hint { Text(hint).font(.caption).foregroundStyle(.tertiary) }
                        }
                        if !command.description.isEmpty {
                            Text(command.description).font(.caption).foregroundStyle(.secondary).lineLimit(2)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 14).padding(.vertical, 8)
                    .contentShape(.rect)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.vertical, 4)
        .glassEffect(.regular, in: .rect(cornerRadius: 18))
    }

    // MARK: Actions

    /// Shrinks a picture to what the daemon takes and uploads it at once,
    /// so sending does not wait on it.
    private func add(_ image: UIImage) {
        guard let prepared = PreparedImage(image) else { return }
        add(prepared)
    }

    private func add(_ prepared: PreparedImage) {
        guard pictures.count < 4 else { return }
        // A terminal takes a picture only with the words it goes with: its
        // path is typed into the prompt then, not as soon as it is chosen.
        if destination == .terminal {
            pictures.append(Picture(thumbnail: prepared.thumbnail, jpeg: prepared.jpeg, progress: UploadProgress(), task: Task { "" }, upload: .sent("")))
            return
        }
        let progress = UploadProgress()
        let picture = Picture(
            thumbnail: prepared.thumbnail,
            jpeg: prepared.jpeg,
            progress: progress,
            task: upload(prepared.jpeg, progress: progress)
        )
        pictures.append(picture)
        let id = picture.id
        Task {
            let result: Picture.Upload
            do {
                result = .sent(try await picture.task.value)
            } catch {
                problem = error.localizedDescription
                result = .failed
            }
            if let index = pictures.firstIndex(where: { $0.id == id }) { pictures[index].upload = result }
        }
    }

    private func upload(_ jpeg: Data, progress: UploadProgress) -> Task<String, Error> {
        let pane = key.pane
        let device = key.device
        let fleet = model.fleet
        progress.fraction = 0
        return Task {
            try await fleet.call(device) { client, token in
                try await client.upload(jpeg, pane: pane, bearer: token) { fraction in
                    Task { @MainActor in progress.fraction = fraction }
                }
            }
        }
    }

    /// The picture's upload id, trying a failed upload once more rather
    /// than sending the prompt without it.
    private func uploaded(_ picture: Picture) async throws -> String {
        if let id = try? await picture.task.value { return id }
        return try await upload(picture.jpeg, progress: picture.progress).value
    }

    /// Keeps the draft on disk for after a relaunch, or removes it once empty.
    private func saveDraft() {
        let saved = draft.isEmpty && pictures.isEmpty ? nil : Cache.Draft(text: draft, pictures: pictures.map(\.jpeg))
        model.fleet.save(draft: saved, for: key, by: id)
    }

    /// The clipboard's pictures, read from the attach menu. Unlike a paste
    /// in the field, iOS may ask before letting the app read them.
    private func pastePhotos() {
        for image in (UIPasteboard.general.images ?? []).prefix(4 - pictures.count) { add(image) }
    }

    /// Sends the shortcut at once when nothing is drafted; otherwise adds
    /// it to the draft, so pictures and typed words go with it.
    private func use(_ shortcut: TextShortcut) {
        if draft.isEmpty && pictures.isEmpty {
            draft = shortcut.text
            sendPrompt()
        } else {
            draft = draft.isEmpty || draft.hasSuffix(" ") || draft.hasSuffix("\n") ? draft + shortcut.text : draft + " " + shortcut.text
            focused = true
        }
    }

    /// Clears the composer and shows the prompt as sending at once; the
    /// pictures still uploading finish before the prompt goes out.
    private func sendPrompt() {
        let text = draft
        let sentPictures = pictures
        sends += 1
        if destination == .terminal {
            sendToTerminal(text, pictures: sentPictures)
            return
        }
        draft = ""
        pictures = []
        dictation.reset()
        saveDraft()
        let outgoing = store.sending(text, uploads: sentPictures.map(\.progress))
        Task {
            do {
                var ids: [String] = []
                for picture in sentPictures { ids.append(try await uploaded(picture)) }
                if !sentPictures.isEmpty { store.uploaded(outgoing) }
                problem = await model.fleet.send(.prompt(text: text, attachments: ids), to: key)
            } catch {
                problem = error.localizedDescription
            }
            store.sent(outgoing, failed: problem != nil)
            if problem != nil {
                // The person may have started another message meanwhile:
                // the failed one goes back in front of it, not over it.
                if draft.isEmpty {
                    draft = text
                } else if !text.isEmpty {
                    draft = text + "\n\n" + draft
                }
                pictures = Array((sentPictures + pictures).prefix(4))
                // Kept on disk too, in case the person left the chat meanwhile.
                saveDraft()
            }
        }
    }

    private func send(_ command: ChatCommand) {
        Task { problem = await model.fleet.send(command, to: key) }
    }

    /// Types the message into the agent's terminal: each picture's path as
    /// a dropped file, the words as one paste, then Enter, in that order.
    /// What fails goes back into the composer.
    private func sendToTerminal(_ text: String, pictures sent: [Picture]) {
        draft = ""
        pictures = []
        dictation.reset()
        saveDraft()
        let pane = key.pane
        let before = terminalSend
        terminalSend = Task {
            await before?.value
            do {
                try await model.fleet.call(key.device) { client, token in
                    for picture in sent { try await client.termUpload(picture.jpeg, pane: pane, bearer: token) }
                    if !text.isEmpty { try await client.termPaste(text, pane: pane, bearer: token) }
                    try await client.termKeys(["enter"], pane: pane, bearer: token)
                }
                problem = nil
            } catch {
                problem = error.localizedDescription
                draft = draft.isEmpty ? text : text + "\n\n" + draft
                pictures = Array((sent + pictures).prefix(4))
                saveDraft()
            }
        }
    }

    private func toTerminal(keys: [String]) {
        let pane = key.pane
        Task {
            do {
                try await model.fleet.call(key.device) { client, token in
                    try await client.termKeys(keys, pane: pane, bearer: token)
                }
            } catch {
                problem = error.localizedDescription
            }
        }
    }
}

/// A chat's unsent words and pictures, kept while the person is away
/// from it (`AppModel.drafts`).
struct ComposerDraft {
    let text: String
    fileprivate let pictures: [Picture]
}

/// A picture waiting in the composer.
private struct Picture: Identifiable {
    enum Upload: Equatable {
        case sending
        case sent(String)
        case failed
    }

    let id = UUID()
    let thumbnail: UIImage
    let jpeg: Data
    let progress: UploadProgress
    let task: Task<String, Error>
    var upload = Upload.sending
}

/// A thin bar for how far an upload or a send has come.
struct ThinProgress: View {
    let value: Double

    var body: some View {
        ProgressView(value: min(max(value, 0), 1))
            .progressViewStyle(.linear)
            .scaleEffect(y: 0.6, anchor: .center)
            .animation(.easeOut(duration: 0.25), value: value)
    }
}

/// The camera, for a photo to send.
struct CameraPicker: UIViewControllerRepresentable {
    let done: (UIImage?) -> Void

    func makeUIViewController(context: Context) -> UIImagePickerController {
        let picker = UIImagePickerController()
        picker.sourceType = .camera
        picker.delegate = context.coordinator
        return picker
    }

    func updateUIViewController(_ picker: UIImagePickerController, context: Context) {}

    func makeCoordinator() -> Coordinator { Coordinator(done: done) }

    @MainActor
    final class Coordinator: NSObject, UIImagePickerControllerDelegate, UINavigationControllerDelegate {
        let done: (UIImage?) -> Void

        init(done: @escaping (UIImage?) -> Void) { self.done = done }

        func imagePickerController(_ picker: UIImagePickerController, didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]) {
            done(info[.originalImage] as? UIImage)
            picker.dismiss(animated: true)
        }

        func imagePickerControllerDidCancel(_ picker: UIImagePickerController) {
            done(nil)
            picker.dismiss(animated: true)
        }
    }
}
