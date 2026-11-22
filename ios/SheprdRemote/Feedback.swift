import AVFoundation
import CoreHaptics
import SheprdRemoteCore
import SwiftUI
import UIKit

/// Whether message effects make sounds and the app gives haptics, as
/// Settings chose. Both are on until turned off.
enum FeedbackSettings {
    static let soundsKey = "effectSounds"
    static let hapticsKey = "haptics"

    static var sounds: Bool { UserDefaults.standard.object(forKey: soundsKey) as? Bool ?? true }
    static var haptics: Bool { UserDefaults.standard.object(forKey: hapticsKey) as? Bool ?? true }
    /// Whether this device can give haptics at all: an iPad cannot.
    static let canTap = CHHapticEngine.capabilitiesForHardware().supportsHaptics
}

/// A touch the app answers with a haptic.
enum Touch {
    /// A message or answer sent.
    case send
    /// A light control, such as a replay link or a terminal key.
    case tap
    /// Something chosen among others.
    case select
    /// A chat stopped.
    case stop
}

/// Plays message effects' sounds and taps (`Cue`), and the haptics of
/// touches, unless Settings turned them off.
///
/// Sounds play in the ambient category: they mix with what else plays,
/// stay quiet with the Ring/Silent switch on silent, and are skipped while
/// dictation or voice holds the audio session, which they would otherwise
/// take from it.
@MainActor
final class Feedback {
    static let shared = Feedback()

    /// Rendered sounds by cue, so an effect renders once. Zap's letters
    /// streaming in make sounds that rarely recur, so the cache is bounded:
    /// past `maxSounds` it starts over.
    private var sounds: [Synth: Data] = [:]
    private static let maxSounds = 32
    /// Kept until they finish: a player let go stops.
    private var players: [AVAudioPlayer] = []
    /// What each chat's screen effect still has to play, stopped when
    /// another in the same chat replaces it: a replayed fireworks must not
    /// keep bursting under the wizz after it. Kept per chat, as an iPad's
    /// windows each show their own.
    private var screens: [ObjectIdentifier: Screen] = [:]
    /// Counts screen effects across chats, so a sound rendered for one
    /// since replaced does not start.
    private var runs = 0

    private struct Screen {
        var run: Int
        var since = Date.now
        var players: [AVAudioPlayer] = []
        var taps: [CHHapticPatternPlayer] = []
    }
    private var engine: CHHapticEngine?
    /// What holds the audio session for the microphone or speech now.
    private var holders: Set<ObjectIdentifier> = []

    /// `owner` takes the audio session for the microphone or speech: no
    /// effect sounds until it lets go.
    func holdAudio(_ owner: AnyObject) {
        holders.insert(ObjectIdentifier(owner))
        for player in players + screens.values.flatMap(\.players) { player.stop() }
        players.removeAll()
        for chat in screens.keys { screens[chat]?.players.removeAll() }
    }

    func releaseAudio(_ owner: AnyObject) {
        holders.remove(ObjectIdentifier(owner))
    }

    /// Plays the sound and taps of effects that just began in `chat`, whose
    /// screen effects replace one another.
    func play(_ began: [ChatEffects.Began], in chat: AnyObject) {
        let chat = ObjectIdentifier(chat)
        for effect in began {
            switch effect.lane {
            case .bubble: play(effect.cue)
            case .newScreen:
                stopScreen(of: chat)
                play(effect.cue, screen: chat)
            case .screen: play(effect.cue, screen: chat)
            }
        }
    }

    /// Plays `cue`, as part of `screen`'s screen effect if given.
    func play(_ cue: Cue, screen chat: ObjectIdentifier? = nil) {
        if FeedbackSettings.haptics, let player = tap(cue.taps), let chat { screens[chat]?.taps.append(player) }
        if FeedbackSettings.sounds, holders.isEmpty { sound(cue.sound, screen: chat.flatMap { chat in screens[chat].map { (chat, $0.run) } }) }
    }

    /// Stops what `chat`'s last screen effect still had to play, and starts
    /// its next.
    private func stopScreen(of chat: ObjectIdentifier) {
        if let last = screens[chat] {
            for player in last.players { player.stop() }
            for player in last.taps { try? player.stop(atTime: CHHapticTimeImmediate) }
        }
        // Chats whose effects are over need nothing kept.
        let longest = ScreenEffect.allCases.map(\.length).max() ?? 0
        screens = screens.filter { Date.now.timeIntervalSince($0.value.since) < longest }
        runs += 1
        screens[chat] = Screen(run: runs)
    }

    /// Answers a touch with a haptic.
    func touch(_ touch: Touch) {
        guard FeedbackSettings.haptics else { return }
        switch touch {
        case .send: UIImpactFeedbackGenerator(style: .medium).impactOccurred()
        case .tap: UIImpactFeedbackGenerator(style: .light).impactOccurred()
        case .select: UISelectionFeedbackGenerator().selectionChanged()
        case .stop: UIImpactFeedbackGenerator(style: .rigid).impactOccurred()
        }
    }

    /// Plays `synth`, as part of a chat's screen effect `run` if given.
    private func sound(_ synth: Synth, screen: (chat: ObjectIdentifier, run: Int)?) {
        guard !synth.voices.isEmpty else { return }
        if let data = sounds[synth] {
            start(data, screen: screen)
            return
        }
        Task {
            // A few seconds of samples: rendered away from the main thread.
            let data = await Task.detached(priority: .userInitiated) { synth.wav() }.value
            if sounds.count >= Self.maxSounds { sounds.removeAll() }
            sounds[synth] = data
            start(data, screen: screen)
        }
    }

    private func start(_ data: Data, screen: (chat: ObjectIdentifier, run: Int)?) {
        // A screen effect replaced while its sound rendered stays quiet.
        guard holders.isEmpty, screen.map({ screens[$0.chat]?.run == $0.run }) ?? true else { return }
        let session = AVAudioSession.sharedInstance()
        do {
            if session.category != .ambient { try session.setCategory(.ambient) }
            try session.setActive(true)
            let player = try AVAudioPlayer(data: data)
            players.removeAll { !$0.isPlaying }
            if let chat = screen?.chat { screens[chat]?.players.append(player) } else { players.append(player) }
            player.play()
        } catch {
            // A sound is a garnish: the effect still shows without it.
        }
    }

    /// Plays `taps`, returning their player so a screen effect's can be
    /// stopped.
    private func tap(_ taps: [Cue.Tap]) -> CHHapticPatternPlayer? {
        guard FeedbackSettings.canTap, !taps.isEmpty else { return nil }
        do {
            let engine = try self.engine ?? makeEngine()
            let events = taps.map { tap in
                CHHapticEvent(
                    eventType: .hapticTransient,
                    parameters: [
                        CHHapticEventParameter(parameterID: .hapticIntensity, value: Float(tap.intensity)),
                        CHHapticEventParameter(parameterID: .hapticSharpness, value: Float(tap.sharpness)),
                    ],
                    relativeTime: tap.at
                )
            }
            try engine.start()
            let player = try engine.makePlayer(with: CHHapticPattern(events: events, parameters: []))
            try player.start(atTime: CHHapticTimeImmediate)
            return player
        } catch {
            engine = nil
            return nil
        }
    }

    private func makeEngine() throws -> CHHapticEngine {
        let engine = try CHHapticEngine()
        // It stops itself when idle and when the app leaves the screen; the
        // next cue starts it again.
        engine.isAutoShutdownEnabled = true
        engine.playsHapticsOnly = true
        engine.resetHandler = { [weak engine] in try? engine?.start() }
        self.engine = engine
        return engine
    }
}

extension View {
    /// `sensoryFeedback`, unless Settings turned haptics off.
    func haptic<T: Equatable>(_ feedback: SensoryFeedback, trigger: T) -> some View {
        modifier(Haptic(feedback: feedback, trigger: trigger) { _, _ in true })
    }

    func haptic<T: Equatable>(_ feedback: SensoryFeedback, trigger: T, condition: @escaping (T, T) -> Bool) -> some View {
        modifier(Haptic(feedback: feedback, trigger: trigger, condition: condition))
    }

    /// `sensoryFeedback` that picks its feedback, unless Settings turned
    /// haptics off.
    func haptic<T: Equatable>(trigger: T, _ feedback: @escaping (T, T) -> SensoryFeedback?) -> some View {
        modifier(ChosenHaptic(trigger: trigger, feedback: feedback))
    }
}

private struct Haptic<T: Equatable>: ViewModifier {
    let feedback: SensoryFeedback
    let trigger: T
    let condition: (T, T) -> Bool
    @AppStorage(FeedbackSettings.hapticsKey) private var enabled = true

    func body(content: Content) -> some View {
        content.sensoryFeedback(feedback, trigger: trigger) { old, new in enabled && condition(old, new) }
    }
}

private struct ChosenHaptic<T: Equatable>: ViewModifier {
    let trigger: T
    let feedback: (T, T) -> SensoryFeedback?
    @AppStorage(FeedbackSettings.hapticsKey) private var enabled = true

    func body(content: Content) -> some View {
        content.sensoryFeedback(trigger: trigger) { old, new in enabled ? feedback(old, new) : nil }
    }
}
