import AVFoundation
import SheprdVoice

/// The choices in Settings > Voice.
enum VoiceSettings {
    static let speaksKey = "voiceSpeaks"
    static let confirmsKey = "voiceConfirms"
    static let languageKey = "voicePhoneLanguage"

    static var speaks: Bool { UserDefaults.standard.object(forKey: speaksKey) as? Bool ?? true }
    /// A spoken yes starts a new worktree, for use without looking.
    static var confirms: Bool { UserDefaults.standard.bool(forKey: confirmsKey) }
    /// Commands in the phone's language rather than English. Only the
    /// model can read those, and less reliably.
    static var phoneLanguage: Bool { UserDefaults.standard.bool(forKey: languageKey) }
    static var english: Bool { !phoneLanguage || Locale.current.language.languageCode == .english }
}


@MainActor
final class VoiceSpeaker: NSObject, AVSpeechSynthesizerDelegate {
    private let synth = AVSpeechSynthesizer()
    private var utterance: AVSpeechUtterance?
    private var player: StreamingSpeech?
    private var download: Task<Void, Error>?
    private var completion: CheckedContinuation<Bool, Never>?
    private var generation = UUID()
    private var ownsAudio = false

    override init() {
        super.init()
        synth.delegate = self
        NotificationCenter.default.addObserver(self, selector: #selector(interrupted(_:)),
            name: AVAudioSession.interruptionNotification, object: AVAudioSession.sharedInstance())
    }

    @objc private nonisolated func interrupted(_ note: Notification) {
        guard note.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt == AVAudioSession.InterruptionType.began.rawValue else { return }
        Task { @MainActor in if self.ownsAudio || self.download != nil { self.stop() } }
    }

    func play(_ words: String, account: SpeechAccount?) async throws -> Bool {
        stop()
        let generation = self.generation
        defer { if self.generation == generation { stop() } }
        if let account {
            let request = try account.speechRequest(words)
            let stream = StreamingSpeech { [weak self] in
                guard let self, self.generation == generation else { throw CancellationError() }
                try self.activateOutput()
            }
            player = stream
            let config = URLSessionConfiguration.ephemeral
            config.timeoutIntervalForRequest = 30
            config.timeoutIntervalForResource = 300
            let http = URLSession(configuration: config, delegate: SpeechHTTP(), delegateQueue: nil)
            defer { http.invalidateAndCancel() }
            let task = Task { try await stream.play(request, session: http) }
            download = task
            do {
                try await withTaskCancellationHandler { try await task.value } onCancel: { task.cancel() }
            } catch where self.generation != generation || task.isCancelled {
                return false
            }
            return self.generation == generation
        }
        try activateOutput()
        return await withCheckedContinuation { continuation in
            completion = continuation
            let utterance = AVSpeechUtterance(string: words)
            self.utterance = utterance
            synth.speak(utterance)
        }
    }

    private func activateOutput() throws {
        let session = AVAudioSession.sharedInstance()
        Feedback.shared.holdAudio(self)
        ownsAudio = true
        try session.setCategory(.playback, options: .duckOthers)
        try session.setActive(true)
    }

    func stop() {
        generation = UUID()
        download?.cancel()
        download = nil
        utterance = nil
        player?.stop()
        player = nil
        synth.stopSpeaking(at: .immediate)
        finish(false)
        if ownsAudio {
            ownsAudio = false
            try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
            Feedback.shared.releaseAudio(self)
        }
    }

    private func finish(_ success: Bool) {
        completion?.resume(returning: success)
        completion = nil
    }

    nonisolated func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) {
        let id = ObjectIdentifier(utterance)
        Task { @MainActor in if self.utterance.map(ObjectIdentifier.init) == id { self.finish(true) } }
    }

    nonisolated func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didCancel utterance: AVSpeechUtterance) {
        let id = ObjectIdentifier(utterance)
        Task { @MainActor in if self.utterance.map(ObjectIdentifier.init) == id { self.finish(false) } }
    }

}
