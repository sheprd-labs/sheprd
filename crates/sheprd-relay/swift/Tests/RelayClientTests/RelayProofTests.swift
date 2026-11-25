import CryptoKit
import Foundation
@testable import RelayClient
import Testing

struct RelayProofTests {
    @Test func routesAreTheHashOfTheCompressedKey() throws {
        let routes = try Vectors.load("routes", as: Vectors.Routes.self)
        #expect(routes.keys.count == 2)
        for key in routes.keys {
            let signing = try P256.Signing.PrivateKey(rawRepresentation: Vectors.hex(key.private))
            #expect(signing.publicKey.compressedRepresentation == (try Vectors.hex(key.public)))
            #expect(Base64URL.encode(signing.publicKey.compressedRepresentation) == key.public_text)
            let route = RelayProof.route(of: signing.publicKey)
            #expect(route == (try Vectors.hex(key.route)))
            #expect(RelayProof.routeText(route) == key.route_text)
        }
    }

    @Test func theContextsAreThePermanentWireBytes() throws {
        let vector = try Vectors.load("websocket", as: Vectors.WebSocket.self)
        #expect(RelayProof.listenContext == (try Vectors.hex(vector.listen_context)))
        #expect(RelayProof.dialContext == (try Vectors.hex(vector.dial_context)))
        let pairing = try Vectors.load("pairing", as: Vectors.Pairing.self)
        #expect(RelayProof.pairingDialContext == (try Vectors.hex(pairing.context)))
    }

    @Test func aListenerSignsTheBytesTheRelayChecks() throws {
        let vector = try Vectors.load("websocket", as: Vectors.WebSocket.self)
        let nonce = try Vectors.hex(vector.nonce)
        #expect(Base64URL.decode(vector.nonce_text) == nonce)
        let signed = RelayProof.listenSigned(name: vector.proof_name, nonce: nonce)
        #expect(signed == (try Vectors.hex(vector.listen_signed)))
        try expectVerifies(key: vector.listener, signature: vector.listen_signature, over: signed)
        try expectHello(vector.listen_hello, key: vector.listener, signed: signed, token: "a listen token")
    }

    @Test func aDialerSignsTheBytesTheRelayChecks() throws {
        let vector = try Vectors.load("websocket", as: Vectors.WebSocket.self)
        let nonce = try Vectors.hex(vector.nonce)
        let route = try Vectors.hex(vector.listener.route)
        let signed = RelayProof.dialSigned(name: vector.proof_name, nonce: nonce, route: route)
        #expect(signed == (try Vectors.hex(vector.dial_signed)))
        try expectVerifies(key: vector.dialer, signature: vector.dial_signature, over: signed)
        try expectHello(vector.dial_hello, key: vector.dialer, signed: signed, token: nil)
    }

    @Test func aLongNameIsCutTo255Bytes() {
        let long = String(repeating: "a", count: 300)
        let signed = RelayProof.listenSigned(name: long, nonce: Data(count: 32))
        #expect(signed.count == RelayProof.listenContext.count + 255 + 1 + 32)
        #expect(signed == RelayProof.listenSigned(name: String(long.prefix(255)), nonce: Data(count: 32)))
    }

    @Test func thePairingDialKeyIsTheOneTheListenerOpens() throws {
        let vector = try Vectors.load("pairing", as: Vectors.Pairing.self)
        let key = RelayProof.pairingDialKey(secret: try Vectors.hex(vector.secret))
        #expect(key.rawRepresentation == (try Vectors.hex(vector.scalar)))
        #expect(key.publicKey.compressedRepresentation == (try Vectors.hex(vector.public)))
        #expect(Base64URL.encode(key.publicKey.compressedRepresentation) == vector.public_text)
        #expect(RelayProof.route(of: key.publicKey) == (try Vectors.hex(vector.route)))
    }

    @Test func base64URLHasNoPaddingAndRefusesStandardAlphabet() {
        #expect(Base64URL.encode(Data([0xfb, 0xff])) == "-_8")
        #expect(Base64URL.decode("-_8") == Data([0xfb, 0xff]))
        #expect(Base64URL.decode("+/8") == nil)
        #expect(Base64URL.decode("-_8=") == nil)
        #expect(Base64URL.decode("A") == nil)
    }

    /// The vector's signature verifies, and so does a fresh one: CryptoKit
    /// randomizes ECDSA, so signatures are never compared byte for byte.
    private func expectVerifies(key: Vectors.Key, signature: String, over signed: Data) throws {
        let publicKey = try P256.Signing.PublicKey(compressedRepresentation: Vectors.hex(key.public))
        let given = try P256.Signing.ECDSASignature(rawRepresentation: Vectors.hex(signature))
        #expect(publicKey.isValidSignature(given, for: signed))
        let signing = try P256.Signing.PrivateKey(rawRepresentation: Vectors.hex(key.private))
        let fresh = try signing.signature(for: signed)
        #expect(fresh.rawRepresentation.count == 64)
        #expect(publicKey.isValidSignature(fresh, for: signed))
    }

    /// The vector's hello and a fresh one carry the same key and token, and
    /// both signatures verify.
    private func expectHello(_ text: String, key: Vectors.Key, signed: Data, token: String?) throws {
        let given = try JSONDecoder().decode(RelayProof.Hello.self, from: Data(text.utf8))
        #expect(given.key == key.public_text)
        #expect(given.token == token)
        let publicKey = try P256.Signing.PublicKey(compressedRepresentation: Vectors.hex(key.public))
        for hello in [given, try fresh(key: key, signed: signed, token: token)] {
            let bytes = try #require(Base64URL.decode(hello.signature))
            let signature = try P256.Signing.ECDSASignature(rawRepresentation: bytes)
            #expect(publicKey.isValidSignature(signature, for: signed))
        }
    }

    private func fresh(key: Vectors.Key, signed: Data, token: String?) throws -> RelayProof.Hello {
        let signing = try P256.Signing.PrivateKey(rawRepresentation: Vectors.hex(key.private))
        let text = try RelayProof.Hello(key: signing, signed: signed, token: token).text()
        // Keys sorted, token left out when absent: the shape the vectors use.
        #expect(text.hasPrefix("{\"key\":\"\(key.public_text)\",\"signature\":\""))
        #expect(text.contains("\"token\"") == (token != nil))
        return try JSONDecoder().decode(RelayProof.Hello.self, from: Data(text.utf8))
    }
}
