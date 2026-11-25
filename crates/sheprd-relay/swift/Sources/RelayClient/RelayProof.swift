import CryptoKit
import Foundation

/// What a listener or dialer proves to a relay over its WebSocket protocol.
///
/// A proof is an ECDSA P-256 signature (`r ‖ s`) over a context, the
/// relay's proof name (``RelayAddress/proofName``, at most 255 bytes), a
/// zero byte, the relay's 32-byte nonce, and, for a dial, the route. The
/// contexts are permanent wire bytes.
public enum RelayProof {
    /// What a listener signs ahead of the name and nonce.
    public static let listenContext = Data("sheprd-relay/listen/v1\0".utf8)
    /// What a dialer signs ahead of the name, nonce, and route.
    public static let dialContext = Data("sheprd-relay/dial/v1\0".utf8)
    /// What a pairing dial key's derivation hashes ahead of the secret.
    public static let pairingDialContext = Data("sheprd-relay/pairing-dial/v1\0".utf8)
    /// The challenge's length in bytes.
    public static let nonceLength = 32
    /// The longest name a proof covers; longer names are cut to this.
    public static let maxNameLength = 255

    /// The bytes a listener signs on the relay `name`.
    public static func listenSigned(name: String, nonce: Data) -> Data {
        signed(context: listenContext, name: name, nonce: nonce, extra: Data())
    }

    /// The bytes a dialer signs to reach `route` on the relay `name`.
    public static func dialSigned(name: String, nonce: Data, route: Data) -> Data {
        signed(context: dialContext, name: name, nonce: nonce, extra: route)
    }

    private static func signed(context: Data, name: String, nonce: Data, extra: Data) -> Data {
        var bytes = context
        bytes.append(contentsOf: Data(name.utf8).prefix(maxNameLength))
        bytes.append(0)
        bytes.append(contentsOf: nonce)
        bytes.append(contentsOf: extra)
        return bytes
    }

    /// The route `key` listens on: SHA-256 of its SEC1 compressed form.
    public static func route(of key: P256.Signing.PublicKey) -> Data {
        Data(SHA256.hash(data: key.compressedRepresentation))
    }

    /// A route as URLs carry it: 43 base64url characters.
    public static func routeText(_ route: Data) -> String {
        Base64URL.encode(route)
    }

    /// The one-dial key a device pairs through a relay with, from a secret
    /// both sides hold: the first SHA-256 of the context, the secret, and a
    /// counter byte (from zero) that is a valid P-256 scalar.
    public static func pairingDialKey(secret: Data) -> P256.Signing.PrivateKey {
        var counter: UInt8 = 0
        while true {
            var input = pairingDialContext
            input.append(contentsOf: secret)
            input.append(counter)
            let digest = Data(SHA256.hash(data: input))
            if let key = try? P256.Signing.PrivateKey(rawRepresentation: digest) { return key }
            // Out of range about once in 2^32 tries; the next counter is not.
            counter &+= 1
        }
    }

    /// A peer's proof, the first text frame it sends: its SEC1 compressed
    /// key and its signature, both base64url without padding, and the
    /// listen token when the relay asks for one (dialers send none).
    public struct Hello: Codable, Equatable, Sendable {
        public var key: String
        public var signature: String
        public var token: String?

        public init(key: String, signature: String, token: String? = nil) {
            self.key = key
            self.signature = signature
            self.token = token
        }

        /// Signs `signed` with `key`.
        public init(key: P256.Signing.PrivateKey, signed: Data, token: String? = nil) throws {
            let signature = try key.signature(for: signed)
            self.init(
                key: Base64URL.encode(key.publicKey.compressedRepresentation),
                signature: Base64URL.encode(signature.rawRepresentation),
                token: token
            )
        }

        /// The JSON text frame, keys sorted, `token` left out when absent.
        public func text() throws -> String {
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
            return String(decoding: try encoder.encode(self), as: UTF8.self)
        }
    }
}

/// Base64url without padding, as the relay writes keys, routes, nonces,
/// and signatures.
enum Base64URL {
    static func encode(_ data: Data) -> String {
        data.base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }

    static func decode(_ text: String) -> Data? {
        guard !text.contains("="), !text.contains("+"), !text.contains("/") else { return nil }
        var base64 = text
            .replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")
        let remainder = base64.count % 4
        if remainder == 1 { return nil }
        if remainder > 0 { base64 += String(repeating: "=", count: 4 - remainder) }
        return Data(base64Encoded: base64)
    }
}
