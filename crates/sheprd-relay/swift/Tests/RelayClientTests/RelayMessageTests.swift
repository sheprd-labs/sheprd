import CryptoKit
import Foundation
@testable import RelayClient
import Testing

struct RelayFramingTests {
    @Test func casesEncodeAndDeframeInEverySplit() throws {
        let vector = try Vectors.load("framing", as: Vectors.Framing.self)
        #expect(vector.max_message == RelayFraming.maxMessage)
        for testCase in vector.cases {
            let messages = try testCase.messages.map(Vectors.hex)
            let encoded = try Vectors.hex(testCase.encoded)
            #expect(try messages.map(RelayFraming.encode).reduce(Data(), +) == encoded)
            for chunk in 1...encoded.count {
                var deframer = RelayFraming.Deframer()
                var read: [Data] = []
                var offset = 0
                while offset < encoded.count {
                    let end = min(offset + chunk, encoded.count)
                    if let message = try deframer.push(encoded.subdata(in: offset..<end)) { read.append(message) }
                    while let message = try deframer.next() { read.append(message) }
                    offset = end
                }
                #expect(read == messages, "chunks of \(chunk)")
                #expect(deframer.buffered == 0)
            }
        }
    }

    @Test func tooLongAMessageIsRefused() throws {
        #expect(try RelayFraming.encode(Data(count: RelayFraming.maxMessage)).count == RelayFraming.maxMessage + 2)
        #expect(throws: RelayFraming.Error.tooLong(length: RelayFraming.maxMessage + 1)) {
            try RelayFraming.encode(Data(count: RelayFraming.maxMessage + 1))
        }
        #expect(try RelayFraming.encode(Data(repeating: 9, count: 258)).prefix(3) == Data([1, 2, 9]))
    }

    @Test func aMessageOverTheCapIsRefusedFromItsPrefix() throws {
        var deframer = RelayFraming.Deframer(cap: 16)
        #expect(throws: RelayFraming.Error.overCap(cap: 16, length: 17)) { try deframer.push(Data([0, 17])) }
        var fits = RelayFraming.Deframer(cap: 16)
        #expect(try fits.push(Data([0, 16])) == nil)
        #expect(fits.wanted == 16)
        #expect(try fits.push(Data(repeating: 1, count: 16)) == Data(repeating: 1, count: 16))
        #expect(RelayFraming.Deframer(cap: 1 << 20).cap == RelayFraming.maxMessage)
    }

    @Test func wantedCountsDownToAWholeMessage() throws {
        var deframer = RelayFraming.Deframer()
        #expect(deframer.wanted == 2)
        #expect(try deframer.push(Data([0])) == nil)
        #expect(deframer.wanted == 1)
        #expect(try deframer.push(Data([3, 1])) == nil)
        #expect(deframer.wanted == 2)
        #expect(try deframer.push(Data([2, 3])) == Data([1, 2, 3]))
        #expect(deframer.wanted == 2)
    }

    @Test func aChunkOfManyShortMessagesDrainsInOrder() throws {
        var chunk = Data()
        for index in 0..<10_000 {
            chunk.append(try RelayFraming.encode(index % 2 == 0 ? Data() : Data([UInt8(index % 251)])))
        }
        // The start of a message the next chunk finishes.
        chunk.append(contentsOf: [0, 3, 7])
        var deframer = RelayFraming.Deframer()
        var messages = [try #require(try deframer.push(chunk))]
        while let message = try deframer.next() { messages.append(message) }
        #expect(messages.count == 10_000)
        #expect(messages.last == Data([UInt8(9_999 % 251)]))
        #expect(deframer.buffered == 3)
        #expect(deframer.wanted == 2)
        #expect(try deframer.push(Data([8, 9])) == Data([7, 8, 9]))
        #expect(deframer.buffered == 0)
    }

    @Test func aLongMessageIsSentInFramesUnderTheRelayLimit() throws {
        let framed = try RelayFraming.encode(Data(repeating: 5, count: RelayFraming.maxMessage))
        let frames = RelayConnection.frames(framed, maximumFrameSize: RelayConnection.defaultFrameSize)
        #expect(frames.count == 5)
        #expect(frames.allSatisfy { $0.count <= RelayConnection.defaultFrameSize })
        #expect(frames.reduce(Data(), +) == framed)
        #expect(RelayConnection.frames(Data([1, 2, 3]), maximumFrameSize: 0) == [Data([1]), Data([2]), Data([3])])
        #expect(RelayConnection.frames(Data(), maximumFrameSize: 4).isEmpty)
    }
}

struct RelayStatusTests {
    @Test func statusesHaveTheRelaysNamesAndBytes() throws {
        let vector = try Vectors.load("messages", as: Vectors.Messages.self)
        #expect(vector.statuses.count == RelayStatus.allCases.count)
        for expected in vector.statuses {
            let status = try JSONDecoder().decode(RelayStatus.self, from: Data("\"\(expected.json)\"".utf8))
            #expect(status.byte == expected.byte)
            #expect(status.rawValue == expected.json)
        }
    }

    @Test func closeCodesMatchTheRelay() throws {
        let vector = try Vectors.load("messages", as: Vectors.Messages.self)
        #expect(vector.close_codes.count == RelayCloseCode.allCases.count)
        for code in RelayCloseCode.allCases {
            #expect(vector.close_codes[code.name] == code.rawValue, "\(code.name)")
        }
    }
}

struct RelayDialerTests {
    @Test func theChallengeTurnsIntoAHelloTheRelayAccepts() throws {
        let messages = try Vectors.load("messages", as: Vectors.Messages.self)
        let vector = try Vectors.load("websocket", as: Vectors.WebSocket.self)
        #expect(try RelayDialer.nonce(challenge: messages.challenge) == (try Vectors.hex(vector.nonce)))
        let address = try RelayAddress("https://relay.example.org/acme")
        let key = try P256.Signing.PrivateKey(rawRepresentation: Vectors.hex(vector.dialer.private))
        let text = try RelayDialer.hello(
            challenge: messages.challenge, proofName: address.proofName,
            route: Vectors.hex(vector.listener.route), key: key
        )
        let hello = try JSONDecoder().decode(RelayProof.Hello.self, from: Data(text.utf8))
        #expect(hello.key == vector.dialer.public_text)
        #expect(hello.token == nil)
        let signature = try P256.Signing.ECDSASignature(rawRepresentation: #require(Base64URL.decode(hello.signature)))
        #expect(key.publicKey.isValidSignature(signature, for: try Vectors.hex(vector.dial_signed)))
    }

    @Test func aChallengeThatIsNotTheProtocolIsRefused() {
        for text in [
            "{\"t\":\"challenge\",\"nonce\":\"AAAA\",\"version\":1}",
            "{\"t\":\"challenge\",\"nonce\":\"Tk5OTk5OTk5OTk5OTk5OTk5OTk5OTk5OTk5OTk5OTk4\",\"version\":2}",
            "{\"t\":\"ready\",\"route\":\"x\"}",
            "not json",
        ] {
            #expect(throws: RelayDialError.protocolViolation, "\(text)") { try RelayDialer.nonce(challenge: text) }
        }
    }

    @Test func statusesMapToOutcomes() throws {
        let messages = try Vectors.load("messages", as: Vectors.Messages.self)
        #expect(try RelayDialer.status(messages.status) == .ok)
        try RelayDialer.check(status: messages.status)
        #expect(throws: RelayDialError.refused(.tooMany)) { try RelayDialer.check(status: messages.status_with_id) }
        #expect(throws: RelayDialError.refused(.offline)) {
            try RelayDialer.check(status: "{\"t\":\"status\",\"status\":\"offline\"}")
        }
        #expect(throws: RelayDialError.unknownStatus("later")) {
            try RelayDialer.check(status: "{\"t\":\"status\",\"status\":\"later\"}")
        }
        #expect(throws: RelayDialError.protocolViolation) { try RelayDialer.check(status: messages.challenge) }
        #expect(try RelayDialer.statusText("{\"t\":\"status\",\"status\":\"later\"}") == "later")
    }

    @Test func aBanIsA403WithEitherHeader() throws {
        let url = try #require(URL(string: "https://relay.example.org/v1/dial/x"))
        func response(_ status: Int, _ headers: [String: String]) throws -> HTTPURLResponse {
            try #require(HTTPURLResponse(url: url, statusCode: status, httpVersion: "HTTP/1.1", headerFields: headers))
        }
        #expect(RelayDialer.isBan(try response(403, ["Relay-Banned": "1"])))
        #expect(RelayDialer.isBan(try response(403, ["sheprd-relay-banned": " 1 "])))
        #expect(!RelayDialer.isBan(try response(403, [:])))
        #expect(!RelayDialer.isBan(try response(403, ["relay-banned": "0"])))
        #expect(!RelayDialer.isBan(try response(429, ["relay-banned": "1"])))
    }
}
