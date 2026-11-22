import Foundation
import Testing
@testable import SheprdRemoteCore

/// The sounds and taps effects play, and when the ledger says they begin.
@Suite struct EffectCueTests {
    private let now = Date(timeIntervalSinceReferenceDate: 1000)

    private var cues: [(String, Cue, length: Double)] {
        BubbleEffect.allCases.map { ($0.rawValue, $0.cue, $0.length) }
            + ScreenEffect.allCases.map { ($0.rawValue, $0.cue(letters: 30), $0.length) }
    }

    private func transcript(_ texts: [String]) -> Transcript {
        var transcript = Transcript(agent: "Claude")
        transcript.entries = texts.enumerated().map { index, text in
            Entry(id: EntryId(UInt64(index + 1)), kind: .agent(text: text))
        }
        return transcript
    }

    @Test func everyCueSoundsAndTapsWithinItsEffect() {
        for (name, cue, length) in cues {
            #expect(!cue.taps.isEmpty, "\(name)")
            for tap in cue.taps {
                #expect((0..<length).contains(tap.at), "\(name)")
                #expect((0...1).contains(tap.intensity) && (0...1).contains(tap.sharpness), "\(name)")
            }
            // A sound may ring a moment past the last frame, no longer.
            #expect(cue.sound.length > 0 && cue.sound.length <= length + 1, "\(name)")
        }
    }

    @Test func soundsStayWithinFullScaleAndAreNotSilent() {
        for (name, cue, _) in cues {
            let samples = cue.sound.samples()
            #expect(samples.count == Int((cue.sound.length * Double(Synth.sampleRate)).rounded(.up)), "\(name)")
            let peak = samples.map(abs).max() ?? 0
            #expect(peak <= 0.95 + 1e-6 && peak > 0.02, "\(name)")
        }
    }

    @Test func aSoundRendersTheSameEachTime() {
        let synth = ScreenEffect.confetti.cue().sound
        #expect(synth.samples() == synth.samples())
    }

    @Test func aWavHoldsItsSamplesBehindAHeader() {
        let synth = Synth([.init(at: 0, length: 0.01, wave: .sine, from: 440, gain: 0.5)])
        let wav = synth.wav()
        #expect(wav.prefix(4) == Data("RIFF".utf8))
        #expect(wav.count == 44 + synth.samples().count * 2)
    }

    @Test func aSoundIsCappedInLength() {
        let synth = Synth([.init(at: 0, length: 60, wave: .noise, from: 500, gain: 0.5)])
        #expect(synth.length == Synth.maxLength)
        #expect(synth.samples().count == Int(Synth.maxLength) * Synth.sampleRate)
    }

    @Test func slamTapsAsItLands() throws {
        let tap = try #require(BubbleEffect.slam.cue.taps.first)
        // It lands when it stops shrinking: smallest there.
        let landed = BubbleEffect.slam.pose(at: tap.at).scale
        #expect(landed < BubbleEffect.slam.pose(at: tap.at - 0.05).scale)
        #expect(landed < BubbleEffect.slam.pose(at: tap.at + 0.05).scale)
    }

    @Test func fireworksTapAsEachRocketBursts() {
        let length = ScreenEffect.fireworks.length
        let bursts = EffectScene.fireworkBursts().filter { $0 < length }
        #expect(ScreenEffect.fireworks.cue().taps.map(\.at) == bursts)
        #expect(!bursts.isEmpty)
    }

    @Test func zapTapsForEachLetterItShootsDown() {
        #expect(ScreenEffect.zap.cue(letters: 5).taps.count == 5)
        #expect(ScreenEffect.zap.cue(letters: 500).taps.count == Zap.maxTargets)
        #expect(ScreenEffect.zap.cue(letters: 5).taps.map(\.at) == (0..<5).map(Zap.hitTime))
    }

    @Test func aWizzShakesThenSettles() {
        let length = ScreenEffect.wizz.length
        #expect(EffectScene.wizzShake(at: -0.1) == .zero)
        #expect(EffectScene.wizzShake(at: length) == .zero)
        var widest = 0.0
        for step in 0..<120 {
            let shift = EffectScene.wizzShake(at: length * Double(step) / 120)
            #expect(abs(shift.width) <= EffectScene.wizzAmplitude && abs(shift.height) <= EffectScene.wizzAmplitude)
            widest = max(widest, abs(shift.width))
        }
        #expect(widest > EffectScene.wizzAmplitude / 2)
        #expect(ScreenEffect(line: "::wizz") == .wizz)
    }

    @Test func liveReportsWhatBeganEachKindOnce() {
        var effects = ChatEffects()
        effects.history(transcript(["earlier"]))
        let texts = ["earlier", ":::slam\na\n:::\n:::slam\nb\n:::\n:::gentle\nc\n:::\n::zap"]
        let all = transcript(texts)
        let began = effects.live(all, changed: all.entries.map(\.id), now: now)
        #expect(began == [.bubble(.slam), .bubble(.gentle), .screen(.zap, text: "a")])
        // The next chunk of the same message begins nothing.
        #expect(effects.live(all, changed: all.entries.map(\.id), now: now.addingTimeInterval(0.1)).isEmpty)
    }

    @Test func historyAndADroppedScreenEffectBeginNothing() {
        var effects = ChatEffects()
        let first = transcript([":::loud\nhi\n:::\n::confetti"])
        effects.history(first)
        #expect(effects.live(first, changed: first.entries.map(\.id), now: now).isEmpty)
        // A second screen effect while one plays is dropped, and silent.
        let both = transcript(["::lasers\none", "::love\ntwo"])
        var fresh = ChatEffects()
        #expect(fresh.live(both, changed: [EntryId(1)], now: now) == [.screen(.lasers, text: "one")])
        #expect(fresh.live(both, changed: [EntryId(2)], now: now.addingTimeInterval(0.5)).isEmpty)
    }

    @Test func zapLettersStreamingInAreShotDownStill() {
        var effects = ChatEffects()
        let early = transcript(["::zap"])
        #expect(effects.live(early, changed: [EntryId(1)], now: now) == [.screen(.zap, text: "")])
        // The words follow half a second later: their shots are still due.
        let later = transcript(["::zap\nhi you"])
        let began = effects.live(later, changed: [EntryId(1)], now: now.addingTimeInterval(0.5))
        #expect(began == [.letters(0..<5, at: 0.5)])
        let cue = began[0].cue
        #expect(cue.taps.map(\.at) == (0..<5).map { Zap.hitTime($0) - 0.5 })
        // Nothing new arrived: nothing more begins.
        #expect(effects.live(later, changed: [EntryId(1)], now: now.addingTimeInterval(0.6)).isEmpty)
    }

    @Test func aCueLaterInKeepsWhatIsStillToCome() {
        let cue = ScreenEffect.fireworks.cue()
        let rest = cue.after(2)
        #expect(rest.taps.map(\.at) == cue.taps.filter { $0.at >= 2 }.map { $0.at - 2 })
        #expect(rest.sound.voices.allSatisfy { $0.at >= 0 })
        #expect(rest.sound.voices.count == cue.sound.voices.filter { $0.at >= 2 }.count)
        #expect(cue.after(0) == cue)
    }

    @Test func aScreenEffectReplacesTheLastOnesSoundAndZapsLettersAddToIt() {
        #expect(ChatEffects.Began.bubble(.slam).lane == .bubble)
        #expect(ChatEffects.Began.screen(.fireworks, text: "").lane == .newScreen)
        #expect(ChatEffects.Began.letters(0..<3, at: 0.2).lane == .screen)
    }

    @Test func aReplayedScreenEffectBegins() {
        var effects = ChatEffects()
        let entry = Entry(id: EntryId(1), kind: .agent(text: "::echo\nhello"))
        #expect(effects.replayScreen(entry, now: now) == .screen(.echo, text: "hello"))
        #expect(effects.replayScreen(Entry(id: EntryId(2), kind: .agent(text: "plain")), now: now) == nil)
    }
}
