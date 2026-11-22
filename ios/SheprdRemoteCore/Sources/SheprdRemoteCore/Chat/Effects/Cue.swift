import Foundation

/// What an effect sounds and feels like as it plays: its sound, and the
/// taps a phone gives, each at the moment its scene or pose shows it, such
/// as a slam landing or a firework bursting.
public struct Cue: Hashable, Sendable {
    /// A tap at `at` seconds in: how strong, and how crisp, both `0...1`.
    public struct Tap: Hashable, Sendable {
        public var at: Double
        public var intensity: Double
        public var sharpness: Double

        public init(at: Double, intensity: Double, sharpness: Double) {
            self.at = at
            self.intensity = intensity
            self.sharpness = sharpness
        }
    }

    public var sound: Synth
    public var taps: [Tap]

    public init(sound: Synth, taps: [Tap]) {
        self.sound = sound
        self.taps = taps
    }

    /// What is left of it `elapsed` seconds in: the voices and taps still
    /// to come, moved to start now.
    public func after(_ elapsed: Double) -> Cue {
        guard elapsed > 0 else { return self }
        return Cue(
            sound: Synth(sound.voices.filter { $0.at >= elapsed }.map { voice in
                var voice = voice
                voice.at -= elapsed
                return voice
            }),
            taps: taps.filter { $0.at >= elapsed }.map { Tap(at: $0.at - elapsed, intensity: $0.intensity, sharpness: $0.sharpness) }
        )
    }
}

private typealias Voice = Synth.Voice

extension BubbleEffect {
    public var cue: Cue {
        switch self {
        case .slam:
            // It lands 30% in, as the pose stops shrinking.
            let land = length * 0.3
            return Cue(
                sound: Synth([
                    Voice(at: land, length: 0.45, wave: .sine, from: 140, to: 38, gain: 0.9),
                    Voice(at: land, length: 0.12, wave: .noise, from: 1_800, to: 300, gain: 0.5),
                ]),
                taps: [Cue.Tap(at: land, intensity: 1, sharpness: 0.35)]
            )
        case .loud:
            // It swells to full size 15% in, then shakes.
            let swell = length * 0.15
            return Cue(
                sound: Synth([
                    Voice(at: 0, length: 0.5, wave: .saw, from: 110, to: 90, gain: 0.35, attack: 0.03),
                    Voice(at: 0, length: 0.5, wave: .square, from: 165, to: 135, gain: 0.2, attack: 0.03),
                    Voice(at: swell, length: 0.15, wave: .noise, from: 2_500, to: 600, gain: 0.35),
                ]),
                taps: [Cue.Tap(at: 0, intensity: 0.6, sharpness: 0.5), Cue.Tap(at: swell, intensity: 0.9, sharpness: 0.6)]
            )
        case .gentle:
            return Cue(
                sound: Synth([
                    Voice(at: 0, length: 1.4, wave: .sine, from: 784, gain: 0.25, attack: 0.25),
                    Voice(at: 0.1, length: 1.2, wave: .sine, from: 1_175, gain: 0.1, attack: 0.25),
                ]),
                taps: [Cue.Tap(at: 0, intensity: 0.3, sharpness: 0.1)]
            )
        case .ink:
            let sparkles = (0..<12).map { Double($0) * 0.07 }
            return Cue(
                sound: Synth(sparkles.enumerated().map { index, at in
                    Voice(at: at, length: 0.12, wave: .sine, from: EffectScene.between(UInt32(index), 61, 2_200, 4_200), gain: 0.1)
                }),
                taps: sparkles.prefix(4).map { Cue.Tap(at: $0 * 2, intensity: 0.25, sharpness: 1) }
            )
        }
    }
}

extension ScreenEffect {
    /// The cue for a message whose words have `letters` letters that are
    /// not spaces, which zap shoots down.
    public func cue(letters: Int = 0) -> Cue {
        switch self {
        case .balloons:
            let pops = [0.0, 0.18, 0.36, 0.6]
            return Cue(
                sound: Synth(pops.enumerated().map { index, at in
                    Voice(at: at, length: 0.16, wave: .sine, from: 330 + Double(index) * 90, to: 520 + Double(index) * 110, gain: 0.35)
                }),
                taps: pops.map { Cue.Tap(at: $0, intensity: 0.4, sharpness: 0.2) }
            )
        case .celebration:
            // Sparks pour for 2.4 seconds: a chime, then a glitter.
            let glitter = (0..<18).map { Double($0) * 0.13 }
            return Cue(
                sound: Synth([
                    Voice(at: 0, length: 1.2, wave: .sine, from: 1_047, gain: 0.25),
                    Voice(at: 0.08, length: 1.2, wave: .sine, from: 1_319, gain: 0.18),
                    Voice(at: 0.16, length: 1.4, wave: .sine, from: 1_568, gain: 0.15),
                ] + glitter.enumerated().map { index, at in
                    Voice(at: at, length: 0.1, wave: .sine, from: EffectScene.between(UInt32(index), 62, 2_600, 4_800), gain: 0.06)
                }),
                taps: [Cue.Tap(at: 0, intensity: 0.6, sharpness: 0.8)]
                    + glitter.prefix(6).map { Cue.Tap(at: $0, intensity: 0.2, sharpness: 1) }
            )
        case .confetti:
            let crackles = (0..<10).map { 0.06 + EffectScene.between(UInt32($0), 63, 0, 0.5) }
            return Cue(
                sound: Synth([
                    Voice(at: 0, length: 0.14, wave: .noise, from: 3_000, to: 800, gain: 0.7),
                    Voice(at: 0, length: 0.12, wave: .sine, from: 300, to: 120, gain: 0.4),
                ] + crackles.map { Voice(at: $0, length: 0.03, wave: .noise, from: 6_000, gain: 0.18) }),
                taps: [Cue.Tap(at: 0, intensity: 0.8, sharpness: 0.9)]
            )
        case .echo:
            let echoes = (0..<5).map(Double.init)
            return Cue(
                sound: Synth(echoes.map { index in
                    Voice(at: index * 0.32, length: 0.7, wave: .sine, from: 660, gain: 0.3 * pow(0.6, index))
                }),
                taps: echoes.prefix(3).map { Cue.Tap(at: $0 * 0.32, intensity: 0.5 * pow(0.6, $0), sharpness: 0.4) }
            )
        case .fireworks:
            let rockets = EffectScene.fireworkBursts().enumerated()
                .map { (launch: Double($0.offset) * 0.33, burst: $0.element) }
                .filter { $0.burst < length }
            let bursts = rockets.map(\.burst)
            return Cue(
                sound: Synth(
                    rockets.map { launch, burst in
                        Voice(at: launch, length: burst - launch, wave: .sine, from: 700, to: 1_600, gain: 0.04, attack: 0.1)
                    } + bursts.flatMap { burst in
                        [
                            Voice(at: burst, length: 0.6, wave: .noise, from: 900, to: 150, gain: 0.55),
                            Voice(at: burst, length: 0.4, wave: .sine, from: 90, to: 40, gain: 0.5),
                        ]
                    }
                ),
                taps: bursts.map { Cue.Tap(at: $0, intensity: 0.85, sharpness: 0.45) }
            )
        case .lasers:
            let shots = (0..<8).map { Double($0) * 0.38 + 0.1 }
            return Cue(
                sound: Synth(shots.map { Voice(at: $0, length: 0.2, wave: .square, from: 1_900, to: 280, gain: 0.14) }),
                taps: shots.map { Cue.Tap(at: $0, intensity: 0.5, sharpness: 1) }
            )
        case .love:
            // The heart beats twice: lub-dub, 20% and 40% in.
            let beats = [0.2, 0.4].map { $0 * length }
            return Cue(
                sound: Synth(beats.flatMap { beat in
                    [
                        Voice(at: beat - 0.12, length: 0.16, wave: .sine, from: 75, to: 50, gain: 0.6),
                        Voice(at: beat, length: 0.2, wave: .sine, from: 65, to: 42, gain: 0.85),
                    ]
                }),
                taps: beats.flatMap { [Cue.Tap(at: $0 - 0.12, intensity: 0.5, sharpness: 0.1), Cue.Tap(at: $0, intensity: 0.9, sharpness: 0.1)] }
            )
        case .spotlight:
            // A stage light clunks on, and settles on the message at 1.2s.
            return Cue(
                sound: Synth([
                    Voice(at: 0, length: 0.08, wave: .noise, from: 2_500, gain: 0.5),
                    Voice(at: 0, length: 0.3, wave: .sine, from: 110, to: 70, gain: 0.5),
                    Voice(at: 0.05, length: 1.3, wave: .saw, from: 60, gain: 0.05, attack: 0.4),
                ]),
                taps: [Cue.Tap(at: 0, intensity: 0.8, sharpness: 0.7), Cue.Tap(at: 1.2, intensity: 0.3, sharpness: 0.2)]
            )
        case .wizz:
            // A buzzer, pulsing while the chat shakes.
            let pulses = (0..<8).map { Double($0) * 0.13 }
            return Cue(
                sound: Synth(pulses.flatMap { at in
                    [
                        Voice(at: at, length: 0.11, wave: .saw, from: 180, gain: 0.45),
                        Voice(at: at, length: 0.11, wave: .square, from: 92, gain: 0.3),
                    ]
                }),
                taps: pulses.map { Cue.Tap(at: $0, intensity: 1, sharpness: 0.6) }
            )
        case .zap:
            return Self.zap(0..<letters)
        }
    }

    /// Zap's sound and taps for the letters in `letters` it shoots down, of
    /// those it has room for: a message streams in after its directive, so
    /// its letters arrive a few at a time.
    public static func zap(_ letters: Range<Int>) -> Cue {
        let hits = letters.clamped(to: 0..<Zap.maxTargets).map(Zap.hitTime)
        return Cue(
            sound: Synth(hits.flatMap { hit in
                [
                    Voice(at: hit - Zap.flight, length: 0.09, wave: .square, from: 1_400, to: 700, gain: 0.08),
                    Voice(at: hit, length: 0.18, wave: .noise, from: 4_000, to: 500, gain: 0.3),
                ]
            }),
            taps: hits.map { Cue.Tap(at: $0, intensity: 0.55, sharpness: 0.9) }
        )
    }
}
