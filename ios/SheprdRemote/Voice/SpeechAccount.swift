import Foundation

enum SpeechProvider: String, CaseIterable, Codable {
    case elevenlabs, openai, deepgram, openrouter

    var name: String {
        switch self {
        case .elevenlabs: "ElevenLabs"
        case .openai: "OpenAI"
        case .deepgram: "Deepgram"
        case .openrouter: "OpenRouter"
        }
    }

    var voices: [String] {
        switch self {
        case .elevenlabs: ["JBFqnCBsd6RMkjVDRZzb"]
        case .openai: ["marin", "cedar", "coral", "alloy", "nova", "sage"]
        case .deepgram: ["aura-2-thalia-en", "aura-2-apollo-en"]
        case .openrouter: ["en_paul_neutral"]
        }
    }
}

struct SpeechAccount: Codable, Equatable {
    let provider: SpeechProvider
    let voice: String
    let key: String

    var valid: Bool {
        (1...4096).contains(key.utf8.count) && key.utf8.allSatisfy { (33...126).contains($0) }
            && (1...96).contains(voice.utf8.count) && voice.utf8.allSatisfy {
                (65...90).contains($0) || (97...122).contains($0) || (48...57).contains($0) || $0 == 45 || $0 == 95
            }
    }

    static func load() throws -> Self? {
        guard let data = try Keychain.read("speaking-voice", group: nil) else { return nil }
        let account = try JSONDecoder().decode(Self.self, from: data)
        guard account.valid else { throw SpeechFailure.invalid }
        return account
    }

    static func save(_ account: Self?) throws {
        guard let account else { return try Keychain.delete("speaking-voice", group: nil) }
        guard account.valid else { throw SpeechFailure.invalid }
        try Keychain.write(JSONEncoder().encode(account), account: "speaking-voice", group: nil)
    }

    func speechRequest(_ words: String) throws -> URLRequest {
        guard valid, words.utf8.count <= 8000 else { throw SpeechFailure.invalid }
        let endpoint: String
        let body: [String: String]
        switch provider {
        case .elevenlabs:
            endpoint = "https://api.elevenlabs.io/v1/text-to-speech/\(voice)/stream"
            body = ["text": words, "model_id": "eleven_multilingual_v2"]
        case .openai, .openrouter:
            endpoint = provider == .openai ? "https://api.openai.com/v1/audio/speech" : "https://openrouter.ai/api/v1/audio/speech"
            body = ["input": words, "model": provider == .openai ? "gpt-4o-mini-tts" : "mistralai/voxtral-mini-tts-2603", "voice": voice, "response_format": "mp3"]
        case .deepgram:
            endpoint = "https://api.deepgram.com/v1/speak?encoding=mp3&model=\(voice)"
            body = ["text": words]
        }
        var request = try request(at: endpoint)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONEncoder().encode(body)
        return request
    }

    struct Choice: Decodable, Identifiable {
        let voice_id: String
        let name: String
        var id: String { voice_id }
    }

    func choices() async throws -> [Choice] {
        struct Page: Decodable { let voices: [Choice] }
        let request = try request(at: "https://api.elevenlabs.io/v2/voices?page_size=100")
        guard provider == .elevenlabs else { throw SpeechFailure.invalid }
        return try await JSONDecoder().decode(Page.self, from: SpeechHTTP.fetch(request, limit: 2 * 1024 * 1024)).voices
    }

    private func request(at endpoint: String) throws -> URLRequest {
        guard valid, let url = URL(string: endpoint) else { throw SpeechFailure.invalid }
        var request = URLRequest(url: url)
        if provider == .elevenlabs { request.setValue(key, forHTTPHeaderField: "xi-api-key") }
        else { request.setValue((provider == .deepgram ? "Token " : "Bearer ") + key, forHTTPHeaderField: "Authorization") }
        return request
    }
}

enum SpeechFailure: LocalizedError {
    case invalid, playback, response(Int)
    var errorDescription: String? {
        switch self {
        case .invalid: "Check the voice ID, API key, and text length."
        case .playback: "Speech playback was interrupted."
        case let .response(status): "The voice provider returned HTTP \(status). Check the account’s key and available credits."
        }
    }
}

final class SpeechHTTP: NSObject, URLSessionTaskDelegate {
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest, completionHandler: @escaping @Sendable (URLRequest?) -> Void) {
        completionHandler(nil)
    }

    static func fetch(_ request: URLRequest, limit: Int) async throws -> Data {
        let config = URLSessionConfiguration.ephemeral
        config.timeoutIntervalForResource = 30
        let session = URLSession(configuration: config, delegate: SpeechHTTP(), delegateQueue: nil)
        defer { session.invalidateAndCancel() }
        let (bytes, response) = try await session.bytes(for: request)
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        guard (200..<300).contains(status) else { throw SpeechFailure.response(status) }
        guard response.expectedContentLength <= limit else { throw SpeechFailure.invalid }
        var data = Data()
        for try await byte in bytes {
            guard data.count < limit else { throw SpeechFailure.invalid }
            data.append(byte)
        }
        return data
    }
}
