import Foundation

/// A sound made of a few voices, drawn as samples rather than shipped as
/// recordings, so each effect's sound follows its scene's own timing and
/// the app carries no audio files. Like a scene, it is a pure function of
/// its voices: the same sound renders the same samples every time.
public struct Synth: Hashable, Sendable {
    /// One voice: a wave gliding from one pitch to another, swelling in
    /// over `attack` and dying away over the rest of its `length`.
    public struct Voice: Hashable, Sendable {
        public enum Wave: Hashable, Sendable {
            case sine
            case square
            case saw
            /// Noise through a low-pass filter whose cutoff glides from
            /// `from` to `to`: a thud low, a hiss high.
            case noise
        }

        public var at: Double
        public var length: Double
        public var wave: Wave
        /// The pitch, or a noise's cutoff, in hertz.
        public var from: Double
        public var to: Double
        public var gain: Double
        public var attack: Double

        public init(at: Double, length: Double, wave: Wave, from: Double, to: Double? = nil, gain: Double, attack: Double = 0.004) {
            self.at = at
            self.length = length
            self.wave = wave
            self.from = from
            self.to = to ?? from
            self.gain = gain
            self.attack = attack
        }
    }

    public static let sampleRate = 44_100
    /// The longest a sound lasts, so a sound never grows without bound.
    public static let maxLength = 6.0

    public var voices: [Voice]

    public init(_ voices: [Voice]) {
        self.voices = voices
    }

    /// How long it lasts, in seconds.
    public var length: Double {
        min(voices.map { $0.at + $0.length }.max() ?? 0, Self.maxLength)
    }

    /// Its samples, mono, each within `-1...1`.
    public func samples() -> [Float] {
        let rate = Double(Self.sampleRate)
        var out = [Float](repeating: 0, count: Int((length * rate).rounded(.up)))
        for (index, voice) in voices.enumerated() {
            let first = max(Int(voice.at * rate), 0)
            let last = min(Int(((voice.at + voice.length) * rate).rounded(.up)), out.count)
            guard first < last, voice.length > 0 else { continue }
            var phase = 0.0
            var low = 0.0
            for sample in first..<last {
                let t = Double(sample - first) / rate
                let k = t / voice.length
                let pitch = voice.from * pow(voice.to / voice.from, k)
                phase += pitch / rate
                phase -= phase.rounded(.down)
                let raw: Double = switch voice.wave {
                case .sine: sin(2 * .pi * phase)
                case .square: phase < 0.5 ? 0.6 : -0.6
                case .saw: (2 * phase - 1) * 0.6
                case .noise: {
                    let white = EffectScene.unit(UInt32(truncatingIfNeeded: sample), UInt32(index) &+ 101) * 2 - 1
                    // A one-pole low-pass, louder for a lower cutoff so a
                    // thud is heard as well as a hiss.
                    let alpha = 1 - exp(-2 * .pi * pitch / rate)
                    low += alpha * (white - low)
                    return low / max(alpha.squareRoot(), 0.05)
                }()
                }
                let swell = voice.attack > 0 ? min(t / voice.attack, 1) : 1
                let decay = exp(-5 * k) * (1 - k)
                out[sample] += Float(raw * voice.gain * swell * decay)
            }
        }
        // Voices that meet may add past full scale: scale the whole sound
        // down rather than clip it.
        let peak = out.map(abs).max() ?? 0
        if peak > 0.95 {
            let scale = 0.95 / peak
            for index in out.indices { out[index] *= scale }
        }
        return out
    }

    /// The sound as a 16-bit mono WAV file, as `AVAudioPlayer` reads one.
    public func wav() -> Data {
        let pcm = samples().map { Int16(max(-1, min(1, $0)) * Float(Int16.max)) }
        let bytes = UInt32(pcm.count * 2)
        var data = Data()
        func put<T: FixedWidthInteger>(_ value: T) {
            withUnsafeBytes(of: value.littleEndian) { data.append(contentsOf: $0) }
        }
        data.append(contentsOf: Array("RIFF".utf8))
        put(36 + bytes)
        data.append(contentsOf: Array("WAVEfmt ".utf8))
        put(UInt32(16))
        put(UInt16(1))
        put(UInt16(1))
        put(UInt32(Self.sampleRate))
        put(UInt32(Self.sampleRate * 2))
        put(UInt16(2))
        put(UInt16(16))
        data.append(contentsOf: Array("data".utf8))
        put(bytes)
        for sample in pcm { put(sample) }
        return data
    }
}
