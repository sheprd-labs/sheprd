import Foundation
import os
import Testing
@testable import SheprdVoice

/// Holds the last half of an MP3 indefinitely unless the test releases it.
private final class HeldResponse: URLProtocol, @unchecked Sendable {
    struct Gate {
        var tail: (@Sendable () -> Void)?
        var cancelled = false
    }
    static let gate = OSAllocatedUnfairLock(initialState: Gate())
    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        do {
            let fixture = Bundle.module.url(forResource: "stream-tone", withExtension: "mp3", subdirectory: "Fixtures")!
            let bytes = try Data(contentsOf: fixture)
            let split = bytes.count / 2
            let url = request.url!
            client?.urlProtocol(self, didReceive: HTTPURLResponse(url: url, statusCode: 200, httpVersion: nil,
                                 headerFields: ["Content-Type": "audio/mpeg"])!, cacheStoragePolicy: .notAllowed)
            Self.gate.withLock { gate in
                gate.tail = { [weak self] in
                    guard let self else { return }
                    self.client?.urlProtocol(self, didLoad: bytes.suffix(from: split))
                    self.client?.urlProtocolDidFinishLoading(self)
                }
            }
            client?.urlProtocol(self, didLoad: bytes.prefix(split))
        } catch { client?.urlProtocol(self, didFailWithError: error) }
    }
    override func stopLoading() {
        Self.gate.withLock { $0.cancelled = true; $0.tail = nil }
    }
}

struct StreamingSpeechTests {
    // The real output queue is muted, but still requires an audio device.
    private static var hasOutputDevice: Bool {
        #if targetEnvironment(simulator)
        true
        #else
        ProcessInfo.processInfo.environment["SHEPRD_AUDIO_TEST"] == "1"
        #endif
    }

    @Test(.enabled(if: hasOutputDevice))
    @MainActor func rendersBeforeEOFAndCancelsAStalledDownload() async throws {
        for cancel in [false, true] {
            HeldResponse.gate.withLock { $0 = .init() }
            let config = URLSessionConfiguration.ephemeral
            config.protocolClasses = [HeldResponse.self]
            let session = URLSession(configuration: config)
            let player = StreamingSpeech(volume: 0)
            let start = ContinuousClock.now
            let task = Task {
                try await player.play(URLRequest(url: URL(string: "https://voice.invalid/speech")!), session: session)
            }
            defer { task.cancel(); player.stop() }
            let deadline = start.advanced(by: .seconds(4))
            while player.returnedBuffers == 0, ContinuousClock.now < deadline {
                try await Task.sleep(for: .milliseconds(10))
            }
            try #require(player.returnedBuffers > 0, "The audio queue must consume a buffer before the server sends EOF")
            print("First output buffer consumed after \(start.duration(to: .now))")
            let tail = try #require(HeldResponse.gate.withLock { $0.tail })
            if cancel {
                let stop = ContinuousClock.now
                task.cancel()
                player.stop()
                await #expect(throws: (any Error).self) { try await task.value }
                #expect(stop.duration(to: .now) < .seconds(1))
                #expect(HeldResponse.gate.withLock { $0.cancelled })
            } else {
                tail()
                try await task.value
            }
        }
    }
}
