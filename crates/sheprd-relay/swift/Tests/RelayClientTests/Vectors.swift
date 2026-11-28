import Foundation
import Testing

/// The relay's Rust vectors, read in place from `../vectors` beside this package.
enum Vectors {
    struct Key: Decodable {
        let `private`: String
        let `public`: String
        let public_text: String
        let route: String
        let route_text: String
    }

    struct Routes: Decodable {
        let keys: [Key]
    }

    struct Endpoint: Decodable {
        let endpoint: String
        let proof_name: String
        let info: String
        let listen: String
        let dial: String
    }

    struct WebSocket: Decodable {
        let dial_context: String
        let dial_hello: String
        let dial_signature: String
        let dial_signed: String
        let dialer: Key
        let endpoints: [Endpoint]
        let listen_context: String
        let listen_hello: String
        let listen_signature: String
        let listen_signed: String
        let listener: Key
        let nonce: String
        let nonce_text: String
        let proof_name: String
    }

    struct Pairing: Decodable {
        let context: String
        let `public`: String
        let public_text: String
        let route: String
        let scalar: String
        let secret: String
    }

    struct Framing: Decodable {
        struct Case: Decodable {
            let encoded: String
            let messages: [String]
        }

        let cases: [Case]
        let max_message: Int
    }

    struct Messages: Decodable {
        struct Status: Decodable {
            let byte: UInt8
            let json: String
        }

        let challenge: String
        let close_codes: [String: Int]
        let status: String
        let status_with_id: String
        let statuses: [Status]
    }

    static func load<T: Decodable>(_ name: String, as type: T.Type = T.self) throws -> T {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent() // RelayClientTests
            .deletingLastPathComponent() // Tests
            .deletingLastPathComponent() // the package
            .deletingLastPathComponent() // the relay crate
            .appendingPathComponent("vectors/\(name).json")
        return try JSONDecoder().decode(T.self, from: Data(contentsOf: url))
    }

    static func hex(_ text: String) throws -> Data {
        var bytes = Data()
        var index = text.startIndex
        while index < text.endIndex {
            let next = text.index(index, offsetBy: 2)
            bytes.append(try #require(UInt8(text[index..<next], radix: 16)))
            index = next
        }
        return bytes
    }

    static func hex(_ data: Data) -> String {
        data.map { String(format: "%02x", $0) }.joined()
    }
}
