import CryptoKit
import Foundation
import RelayClient

/// What a dialer proves to a relay over its WebSocket fallback, as
/// `sheprd-relay`'s `socket` module checks it. The bytes come from the
/// relay's own Swift client, `RelayClient`; this keeps the app's API.
public enum RelayProof {
    static let dialContext = RelayClient.RelayProof.dialContext
    public static let nonceLength = RelayClient.RelayProof.nonceLength

    /// The bytes a dialer signs to reach `route` on the relay `name`.
    public static func dialSigned(name: String, nonce: Data, route: Data) -> Data {
        RelayClient.RelayProof.dialSigned(name: name, nonce: nonce, route: route)
    }

    static let pairingDialContext = RelayClient.RelayProof.pairingDialContext

    /// The one-dial key a phone pairs through a relay with, from the
    /// pairing secret: as `sheprd-relay`'s `pairing_dial_key`, the first
    /// SHA-256 of context, secret, and a counter that is a P-256 scalar.
    public static func pairingDialKey(secret: Data) -> P256.Signing.PrivateKey {
        RelayClient.RelayProof.pairingDialKey(secret: secret)
    }

    /// The dialer's hello: its key, SEC1 compressed, and its signature,
    /// `r ‖ s`, both base64url.
    public static func hello(key: P256.Signing.PrivateKey, signed: Data) throws -> [String: String] {
        let hello = try RelayClient.RelayProof.Hello(key: key, signed: signed)
        return ["key": hello.key, "signature": hello.signature]
    }
}
