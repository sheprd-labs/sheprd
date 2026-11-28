import Foundation

/// Where an agent interaction should surface on desktop and iOS.
public enum InteractionScope: String, Codable, Hashable, Sendable {
    case window
    case all
}

/// Semantic audio + CoreHaptics + edge-glow cue (`::cue yubikey`).
public enum InteractionCue: String, CaseIterable, Codable, Hashable, Sendable {
    // keep-sorted start
    case countdown
    case decision
    case done
    case secret
    case yubikey
    // keep-sorted end

    public init?(line: String) {
        let indent = line.prefix { $0 == " " || $0 == "\t" }
        guard indent.count <= 3, !indent.contains("\t") else { return nil }
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        guard trimmed.hasPrefix("::"), !trimmed.hasPrefix(":::") else { return nil }
        let rest = String(trimmed.dropFirst(2))
        guard rest.hasPrefix("cue ") else { return nil }
        let name = rest.dropFirst(4).trimmingCharacters(in: .whitespaces)
        guard let parsed = Self(rawValue: name) else { return nil }
        self = parsed
    }

    /// Synthesized tone and CoreHaptics tap pattern for iPhone & iPad.
    public var cue: Cue {
        switch self {
        case .yubikey:
            Cue(
                sound: Synth([
                    .init(at: 0, length: 0.09, wave: .sine, from: 240, gain: 0.45),
                    .init(at: 0.14, length: 0.11, wave: .sine, from: 320, gain: 0.5),
                ]),
                taps: [
                    .init(at: 0, intensity: 0.85, sharpness: 0.7),
                    .init(at: 0.14, intensity: 0.95, sharpness: 0.8),
                ]
            )
        case .decision:
            Cue(
                sound: Synth([
                    .init(at: 0, length: 0.16, wave: .sine, from: 523.25, gain: 0.3),
                    .init(at: 0.12, length: 0.22, wave: .sine, from: 659.25, gain: 0.3),
                ]),
                taps: [
                    .init(at: 0, intensity: 0.45, sharpness: 0.4),
                    .init(at: 0.12, intensity: 0.55, sharpness: 0.5),
                ]
            )
        case .secret:
            Cue(
                sound: Synth([
                    .init(at: 0, length: 0.13, wave: .sine, from: 784, gain: 0.25),
                    .init(at: 0.09, length: 0.20, wave: .sine, from: 1046.5, gain: 0.25),
                ]),
                taps: [
                    .init(at: 0, intensity: 0.5, sharpness: 0.9),
                    .init(at: 0.09, intensity: 0.65, sharpness: 0.95),
                ]
            )
        case .countdown:
            Cue(
                sound: Synth([
                    .init(at: 0, length: 0.05, wave: .square, from: 880, gain: 0.2),
                    .init(at: 0.28, length: 0.05, wave: .square, from: 880, gain: 0.2),
                ]),
                taps: [
                    .init(at: 0, intensity: 0.55, sharpness: 0.85),
                    .init(at: 0.28, intensity: 0.55, sharpness: 0.85),
                ]
            )
        case .done:
            Cue(
                sound: Synth([
                    .init(at: 0, length: 0.22, wave: .sine, from: 523.25, gain: 0.25),
                    .init(at: 0.07, length: 0.22, wave: .sine, from: 659.25, gain: 0.25),
                    .init(at: 0.14, length: 0.28, wave: .sine, from: 783.99, gain: 0.28),
                ]),
                taps: [
                    .init(at: 0, intensity: 0.5, sharpness: 0.5),
                    .init(at: 0.14, intensity: 0.75, sharpness: 0.7),
                ]
            )
        }
    }
}

/// One step in a shared human + agent runbook checklist.
public struct RunbookStep: Codable, Hashable, Sendable {
    public var label: String
    public var human: Bool
    public var done: Bool

    public init(label: String, human: Bool, done: Bool) {
        self.label = label
        self.human = human
        self.done = done
    }
}

/// The block directive keyword that opened an interaction block.
public enum InteractionDirective: String, CaseIterable, Codable, Hashable, Sendable {
    // keep-sorted start
    case countdown
    case diff
    case drop
    case modal
    case oauth
    case otp
    case progress
    case runbook
    case secret
    case spotlight
    case urgent
    // keep-sorted end

    public var defaultScope: InteractionScope {
        switch self {
        case .urgent, .otp, .oauth, .progress, .drop: .all
        case .modal, .countdown, .secret, .runbook, .diff, .spotlight: .window
        }
    }

    public var defaultCue: InteractionCue? {
        switch self {
        case .urgent, .otp, .oauth: .yubikey
        case .modal, .diff: .decision
        case .secret: .secret
        case .countdown: .countdown
        case .progress, .runbook, .drop, .spotlight: nil
        }
    }

    public var defaultAutoClear: Bool {
        switch self {
        case .progress, .runbook: false
        default: true
        }
    }

    public var defaultTitle: String {
        switch self {
        case .modal: "Agent needs your attention"
        case .urgent: "Action required"
        case .countdown: "Confirm scheduled action"
        case .otp: "Enter 2FA code"
        case .oauth: "Complete sign-in"
        case .secret: "Provide secret securely"
        case .progress: "Working…"
        case .runbook: "Shared checklist"
        case .drop: "Send a file to the agent"
        case .diff: "Approve change?"
        case .spotlight: "Agent highlight"
        }
    }
}

/// Typed payload carried inside an [`AgentInteraction`].
public enum InteractionPayload: Hashable, Sendable {
    case modal(preset: String?, choices: [String], quickReply: Bool)
    case countdown(seconds: Int)
    case otp
    case oauth(url: String, code: String?)
    case secret(key: String, dest: String, saveKeychain: Bool)
    case progress(percent: Int?, step: String, done: Bool)
    case runbook(steps: [RunbookStep])
    case drop(accept: String)
    case diff(file: String, diff: String)
    case spotlight(target: String, step: String?)
}

/// A parsed persistent agent interaction (`:::modal`, `:::urgent`, `:::secret`,
/// `:::progress`, `:::runbook`, `:::drop`, `:::diff`, `:::spotlight`, etc.).
public struct AgentInteraction: Hashable, Sendable {
    public var directive: InteractionDirective
    public var scope: InteractionScope
    public var title: String
    public var body: String
    public var cue: InteractionCue?
    public var autoClear: Bool
    public var payload: InteractionPayload

    public init(
        directive: InteractionDirective,
        scope: InteractionScope,
        title: String,
        body: String,
        cue: InteractionCue?,
        autoClear: Bool,
        payload: InteractionPayload
    ) {
        self.directive = directive
        self.scope = scope
        self.title = title
        self.body = body
        self.cue = cue
        self.autoClear = autoClear
        self.payload = payload
    }

    /// Parses an opening line such as `:::urgent Tap your YubiKey`.
    public static func opening(_ line: String) -> (InteractionDirective, String)? {
        let indent = line.prefix { $0 == " " || $0 == "\t" }
        guard indent.count <= 3, !indent.contains("\t") else { return nil }
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        guard trimmed.hasPrefix(":::") else { return nil }
        let rest = String(trimmed.dropFirst(3))
        guard !rest.isEmpty else { return nil }
        let parts = rest.split(maxSplits: 1, omittingEmptySubsequences: true, whereSeparator: \.isWhitespace)
        guard let first = parts.first, let directive = InteractionDirective(rawValue: String(first)) else {
            return nil
        }
        let title = parts.count > 1 ? String(parts[1]).trimmingCharacters(in: .whitespaces) : ""
        return (directive, String(title.prefix(200)))
    }

    /// Parses the inner lines of an interaction block.
    public static func parse(directive: InteractionDirective, title rawTitle: String, inner: String) -> Self {
        var scope = directive.defaultScope
        var cue = directive.defaultCue
        var autoClear = directive.defaultAutoClear
        var preset: String?
        var choices: [String] = []
        var quickReply = false
        var seconds = 15
        var url = ""
        var code: String?
        var key = ""
        var dest = ""
        var saveKeychain = true
        var percent: Int?
        var step = ""
        var done = false
        var accept = ""
        var file = ""
        var target = ""
        var stepLabel: String?

        var bodyLines: [String] = []
        var steps: [RunbookStep] = []
        var inHeaders = true

        for raw in inner.split(separator: "\n", omittingEmptySubsequences: false).map(String.init) {
            let trimmed = raw.trimmingCharacters(in: .whitespaces)
            if directive == .runbook, let item = parseRunbookLine(trimmed) {
                if steps.count < 32 { steps.append(item) }
                inHeaders = false
                continue
            }
            if inHeaders, let (k, v) = splitHeader(trimmed) {
                switch k {
                case "scope":
                    if v == "all" || v == "global" { scope = .all }
                    else if v == "window" { scope = .window }
                    continue
                case "cue":
                    if let parsed = InteractionCue(rawValue: v) { cue = parsed }
                    continue
                case "auto_clear":
                    autoClear = ["true", "1", "yes"].contains(v)
                    continue
                case "preset":
                    preset = v.isEmpty ? nil : String(v.prefix(32))
                    continue
                case "choices":
                    choices = v.split(separator: "|")
                        .map { $0.trimmingCharacters(in: .whitespaces) }
                        .filter { !$0.isEmpty }
                        .prefix(8)
                        .map { String($0.prefix(80)) }
                    continue
                case "reply", "quick_reply":
                    quickReply = ["true", "1", "yes"].contains(v)
                    continue
                case "seconds":
                    if let n = Int(v) { seconds = min(300, max(1, n)) }
                    continue
                case "url":
                    url = String(v.prefix(512))
                    continue
                case "code":
                    code = v.isEmpty ? nil : String(v.prefix(64))
                    continue
                case "key":
                    key = String(v.prefix(128))
                    continue
                case "dest":
                    dest = String(v.prefix(256))
                    continue
                case "save_keychain":
                    saveKeychain = ["true", "1", "yes"].contains(v)
                    continue
                case "percent":
                    let clean = v.hasSuffix("%") ? String(v.dropLast()) : v
                    if let n = Int(clean) { percent = min(100, max(0, n)) }
                    continue
                case "step":
                    if directive == .spotlight {
                        stepLabel = v.isEmpty ? nil : String(v.prefix(32))
                    } else {
                        step = String(v.prefix(160))
                    }
                    continue
                case "done":
                    done = ["true", "1", "yes"].contains(v)
                    if done { cue = .done }
                    continue
                case "accept":
                    accept = String(v.prefix(120))
                    continue
                case "file":
                    file = String(v.prefix(256))
                    continue
                case "target":
                    target = String(v.prefix(160))
                    continue
                default:
                    break
                }
            }
            if !trimmed.isEmpty { inHeaders = false }
            bodyLines.append(raw)
        }

        let body = String(bodyLines.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines).prefix(4096))
        let trimmedTitle = rawTitle.trimmingCharacters(in: .whitespaces)
        let title = trimmedTitle.isEmpty ? directive.defaultTitle : String(trimmedTitle.prefix(200))

        let payload: InteractionPayload = switch directive {
        case .modal, .urgent: .modal(preset: preset, choices: choices, quickReply: quickReply)
        case .countdown: .countdown(seconds: seconds)
        case .otp: .otp
        case .oauth: .oauth(url: url, code: code)
        case .secret: .secret(key: key, dest: dest, saveKeychain: saveKeychain)
        case .progress: .progress(percent: percent, step: step, done: done)
        case .runbook: .runbook(steps: steps)
        case .drop: .drop(accept: accept)
        case .diff: .diff(file: file, diff: body)
        case .spotlight: .spotlight(target: target, step: stepLabel)
        }

        return Self(
            directive: directive,
            scope: scope,
            title: title,
            body: directive == .diff ? "" : body,
            cue: cue,
            autoClear: autoClear,
            payload: payload
        )
    }

    private static func splitHeader(_ line: String) -> (String, String)? {
        guard let colon = line.firstIndex(of: ":") else { return nil }
        let key = line[..<colon].trimmingCharacters(in: .whitespaces)
        guard !key.isEmpty, key.unicodeScalars.allSatisfy({ ($0.value >= 97 && $0.value <= 122) || $0 == "_" }) else {
            return nil
        }
        let val = line[line.index(after: colon)...].trimmingCharacters(in: .whitespaces)
        return (key, val)
    }

    private static func parseRunbookLine(_ line: String) -> RunbookStep? {
        let rest: String
        if line.hasPrefix("- ") || line.hasPrefix("* ") {
            rest = String(line.dropFirst(2))
        } else {
            return nil
        }
        let done: Bool
        let afterBox: String
        if rest.hasPrefix("[x] ") || rest.hasPrefix("[X] ") {
            done = true
            afterBox = String(rest.dropFirst(4))
        } else if rest.hasPrefix("[ ] ") {
            done = false
            afterBox = String(rest.dropFirst(4))
        } else {
            return nil
        }
        let human: Bool
        let label: String
        if afterBox.hasPrefix("you:") {
            human = true
            label = String(afterBox.dropFirst(4)).trimmingCharacters(in: .whitespaces)
        } else if afterBox.hasPrefix("human:") {
            human = true
            label = String(afterBox.dropFirst(6)).trimmingCharacters(in: .whitespaces)
        } else if afterBox.hasPrefix("agent:") {
            human = false
            label = String(afterBox.dropFirst(6)).trimmingCharacters(in: .whitespaces)
        } else {
            human = true
            label = afterBox.trimmingCharacters(in: .whitespaces)
        }
        guard !label.isEmpty else { return nil }
        return RunbookStep(label: String(label.prefix(180)), human: human, done: done)
    }
}
