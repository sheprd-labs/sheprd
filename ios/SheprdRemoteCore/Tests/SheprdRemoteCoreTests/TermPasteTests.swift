import Testing
@testable import SheprdRemoteCore

@Suite struct TermPasteTests {
    @Test func shortTextIsOnePaste() {
        #expect(TermInput.pastes(for: "hello\nworld") == [.paste("hello\nworld")])
        #expect(TermInput.pastes(for: "") == [])
    }

    @Test func longTextSplitsAfterALineAndArrivesWhole() {
        let text = "one\ntwo\nthree"
        let sends = TermInput.pastes(for: text, limit: 9)
        #expect(sends == [.paste("one\ntwo\n"), .paste("three")])
    }

    @Test func aLineLongerThanTheLimitSplitsBetweenCharacters() {
        // "é" is two bytes and the flag eight: neither is ever cut.
        let text = "ééé🇫🇷x"
        let sends = TermInput.pastes(for: text, limit: 9)
        #expect(sends == [.paste("ééé"), .paste("🇫🇷x")])
        let joined = sends.map { send -> String in
            guard case let .paste(part) = send else { return "" }
            #expect(part.utf8.count <= 9)
            return part
        }.joined()
        #expect(joined == text)
    }

    @Test func aCharacterLongerThanTheLimitSplitsBetweenScalarsAndArrivesWhole() {
        // One character: "e" and twenty combining acute accents, 41 bytes.
        let huge = "e" + String(repeating: "\u{301}", count: 20)
        let text = "a" + huge + "b"
        #expect(huge.count == 1)
        let sends = TermInput.pastes(for: text, limit: 9)
        #expect(sends.count > 1)
        var joined = ""
        for case let .paste(part) in sends {
            #expect(part.utf8.count <= 9)
            joined += part
        }
        #expect(joined.unicodeScalars.elementsEqual(text.unicodeScalars))
    }

    @Test func everyPasteFitsTheDaemonsLimit() {
        let text = String(repeating: "a line of text\n", count: 20_000)
        let sends = TermInput.pastes(for: text)
        #expect(sends.count > 1)
        var joined = ""
        for case let .paste(part) in sends {
            #expect(part.utf8.count <= TermInput.maxPaste)
            #expect(part.hasSuffix("\n"))
            joined += part
        }
        #expect(joined == text)
    }
}
