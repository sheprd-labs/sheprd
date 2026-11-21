import CryptoKit
import DeviceCheck
import Foundation
import SheprdRemoteCore

/// This phone's side of the Sheprd push relay (`docs/apns-relay-plan.md`),
/// in the main app only: it attests with App Attest for a credential, keeps
/// it in the app's own keychain item, attests again a week before it runs
/// out, and makes handles, each a push token sealed with the credential to
/// the push relay's key, which the computers get in place of tokens.
///
/// Nothing here runs while the setting is off: no attestation, no call to
/// the push relay, no handle. Turning it off revokes the handles this phone
/// made and forgets the credential.
@MainActor
@Observable
final class PushRelayAccess {
    enum Failure: Error, LocalizedError {
        /// App Attest is not available, as on the simulator.
        case unsupported
        /// The push relay answered with an error status and its outcome.
        case refused(status: Int, outcome: String?)
        case malformed

        var errorDescription: String? {
            switch self {
            case .unsupported:
                "This device cannot prove the app is genuine (App Attest), which the push relay needs."
            case let .refused(status, outcome):
                "The push relay refused this phone (\(outcome ?? "HTTP \(status)"))."
            case .malformed:
                "The push relay sent an answer this app cannot read."
            }
        }
    }

    /// What the app keeps: the credential, and the handles it made with
    /// it, so they can be reused while their token lasts and revoked when
    /// the setting goes off.
    private struct Stored: Codable {
        var credential: PushRelayCredential?
        /// Newest last, at most `maxHandles`.
        var handles: [Made] = []
    }

    private struct Made: Codable {
        /// A hash of the kind, token, and credential, so the token itself
        /// is not kept beside its handle.
        let key: String
        let handle: String
    }

    private static let account = "push-relay"
    /// More than the tokens one phone has at once: a push-to-start token,
    /// an alert token, and a few activities.
    private static let maxHandles = 64

    /// This build's push relay: the sandbox one for Xcode builds.
    let environment = PushRelayEnvironment(
        provisioningProfile: Bundle.main.url(forResource: "embedded", withExtension: "mobileprovision")
            .flatMap { try? Data(contentsOf: $0) }
    )
    private(set) var lastError: String?
    private var stored: Stored
    private var attesting: Task<PushRelayCredential, any Error>?

    init() {
        stored = (try? Keychain.read(Self.account, group: nil))
            .flatMap { $0 }
            .flatMap { try? JSONDecoder().decode(Stored.self, from: $0) } ?? Stored()
    }

    var credential: PushRelayCredential? { stored.credential }

    /// A credential to make handles with, attesting first when there is
    /// none or it nears its end. `nil` while the setting is off, or when
    /// attesting failed and no credential is still good.
    func usableCredential(at now: Date = Date()) async -> PushRelayCredential? {
        guard PushRelaySetting.isOn else { return nil }
        let current = stored.credential.flatMap { $0.environment == environment ? $0 : nil }
        if let current, !current.needsRenewal(at: now) { return current }
        do {
            let task = attesting ?? Task { try await attest() }
            attesting = task
            defer { attesting = nil }
            let credential = try await task.value
            // The setting may have gone off while attesting.
            guard PushRelaySetting.isOn else { return nil }
            stored = Stored(credential: credential)
            save()
            lastError = nil
            return credential
        } catch {
            lastError = error.localizedDescription
            return current.flatMap { $0.isValid(at: now) ? $0 : nil }
        }
    }

    /// The handle for `token` of `kind` under `credential`: the one made
    /// before, or a new one.
    func handle(for token: String, kind: PushRelayKind, credential: PushRelayCredential) -> String? {
        let digest = SHA256.hash(data: Data("\(kind.rawValue)|\(token.lowercased())|\(credential.credential)".utf8))
        let key = Base64URL.encode(Data(digest))
        if let made = stored.handles.first(where: { $0.key == key }) { return made.handle }
        guard let handle = try? credential.handle(token: token, kind: kind) else { return nil }
        stored.handles.append(Made(key: key, handle: handle))
        stored.handles.removeFirst(max(0, stored.handles.count - Self.maxHandles))
        save()
        return handle
    }

    /// The setting went off: revokes every handle this phone made, so the
    /// push relay refuses them whichever computer still holds one, then
    /// forgets them and the credential. Best effort: a handle the push
    /// relay could not be told about still stops when its credential runs
    /// out, at most 30 days on, and the computers get tokens instead.
    func turnedOff() async {
        let made = stored.handles
        let environment = stored.credential?.environment ?? environment
        stored = Stored()
        try? Keychain.delete(Self.account, group: nil)
        lastError = nil
        for handle in made {
            do {
                let _: Empty = try await post(environment, "/v1/revoke", body: ["handle": handle.handle])
            } catch Failure.refused(status: 429, _) {
                return
            } catch {
                continue
            }
        }
    }

    private func save() {
        do {
            try Keychain.write(JSONEncoder().encode(stored), account: Self.account, group: nil)
        } catch {
            lastError = error.localizedDescription
        }
    }

    /// `POST /v1/challenge`, App Attest over its SHA-256, then
    /// `POST /v1/attest`. Each challenge works once, and a new App Attest
    /// key is made each time, as renewal by assertion is not built yet.
    private func attest() async throws -> PushRelayCredential {
        let service = DCAppAttestService.shared
        guard service.isSupported else { throw Failure.unsupported }
        let environment = environment
        let challenge: Challenge = try await post(environment, "/v1/challenge", body: [String: String]())
        let keyId = try await service.generateKey()
        let hash = Data(SHA256.hash(data: Data(challenge.challenge.utf8)))
        let attestation = try await service.attestKey(keyId, clientDataHash: hash)
        let issued: PushRelayIssued = try await post(environment, "/v1/attest", body: [
            "attestation": attestation.base64EncodedString(),
            "challenge": challenge.challenge,
            "key_id": keyId,
        ])
        return try PushRelayCredential(issued, environment: environment)
    }

    private struct Challenge: Decodable {
        let challenge: String
    }

    private struct Empty: Decodable {}

    private struct Outcome: Decodable {
        let outcome: String
    }

    /// The push relay is on the public web with a public certificate, so
    /// the system's trust applies, not the computers' pinning.
    private func post<Answer: Decodable>(
        _ environment: PushRelayEnvironment,
        _ path: String,
        body: [String: String]
    ) async throws -> Answer {
        guard let url = environment.url(path) else { throw Failure.malformed }
        var request = URLRequest(url: url, timeoutInterval: 20)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONEncoder().encode(body)
        let (data, response) = try await URLSession.shared.data(for: request)
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        guard (200 ..< 300).contains(status) else {
            throw Failure.refused(status: status, outcome: try? JSONDecoder().decode(Outcome.self, from: data).outcome)
        }
        if Answer.self == Empty.self, let empty = Empty() as? Answer { return empty }
        guard let answer = try? JSONDecoder().decode(Answer.self, from: data) else { throw Failure.malformed }
        return answer
    }
}
