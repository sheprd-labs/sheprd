import Foundation

/// Stable destinations shared with the daemon, independent of language or layout.
public enum AgentSettingsPage: String, Codable, CaseIterable, Hashable, Sendable {
    case chats, computers, delivery, fleet, github, notifications, privacy, relays, sounds, voice
    case deskHold = "desk-hold"
    case terminalKeys = "terminal-keys"
    case textShortcuts = "text-shortcuts"

    public var path: [Self] {
        switch self {
        case .delivery, .deskHold: [.notifications, self]
        case .textShortcuts: [.chats, self]
        default: [self]
        }
    }
}

public enum AgentHighlight: String, Codable, CaseIterable, Hashable, Sendable {
    case chatRows = "chat-rows"
    case faceId = "face-id"
    case haptics
    case liveActivities = "live-activities"
    case pushRelay = "push-relay"
    case sounds
    case speakResults = "speak-results"
    case storage
    case testPush = "test-push"
    case voiceConfirmation = "voice-confirmation"
    case voiceLanguage = "voice-language"

    public var page: AgentSettingsPage {
        switch self {
        case .chatRows: .chats
        case .faceId, .storage: .privacy
        case .haptics, .sounds: .sounds
        case .liveActivities, .pushRelay, .testPush: .notifications
        case .speakResults, .voiceConfirmation, .voiceLanguage: .voice
        }
    }
}
