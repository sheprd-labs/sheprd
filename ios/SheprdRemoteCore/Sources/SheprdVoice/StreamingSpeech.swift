import AudioToolbox
import Foundation

/// Plays a forward-only MP3 response. The parser and output queue live on the
/// main actor; byte collection runs off it and waits when the queue is full.
@MainActor
public final class StreamingSpeech {
    public enum Failure: LocalizedError {
        case response(Int), invalid, audio(OSStatus)
        public var errorDescription: String? {
            switch self {
            case let .response(code): "The voice provider returned HTTP \(code)."
            case .invalid: "The voice provider returned invalid or oversized audio."
            case let .audio(status): "Speech playback failed (\(status))."
            }
        }
    }

    private let mp3 = MP3Packets()
    private var connection: URLSession?
    // Immediate Stop waits for pending Core Audio callbacks. Keep those off
    // the actor that calls Stop, including when the device is interrupted.
    private static let callbacks = DispatchQueue(label: "org.sheprd.speech.output")
    private var queue: AudioQueueRef?
    private var buffers: [AudioQueueBufferRef] = []
    private var allocated = 0
    private var stopped = false
    private(set) var started = false
    private(set) var returnedBuffers = 0
    private let volume: Float
    private let activateOutput: @MainActor () throws -> Void

    public init(activateOutput: @escaping @MainActor () throws -> Void = {}) {
        volume = 1
        self.activateOutput = activateOutput
    }

    init(volume: Float) {
        self.volume = volume
        activateOutput = {}
    }

    /// Supply a dedicated session. Stopping playback also cancels its request.
    public func play(_ request: URLRequest, session: URLSession) async throws {
        defer { stop() }
        guard !stopped else { throw CancellationError() }
        connection = session
        try mp3.open()
        try await Self.receive(request, session: session, player: self)
        try Task.checkCancellation()
        guard !stopped, started, let queue else { throw Failure.invalid }
        // An asynchronous stop drains the hardware, including its final buffer,
        // before a caller may enable the microphone for a spoken confirmation.
        try check(AudioQueueStop(queue, false))
        while !stopped {
            var running: UInt32 = 0
            var size = UInt32(MemoryLayout.size(ofValue: running))
            try check(AudioQueueGetProperty(queue, kAudioQueueProperty_IsRunning, &running, &size))
            if running == 0 { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        throw CancellationError()
    }

    private nonisolated static func receive(_ request: URLRequest, session: URLSession,
                                            player: StreamingSpeech) async throws {
        try await withTaskCancellationHandler {
            let (bytes, response) = try await session.bytes(for: request)
            let status = (response as? HTTPURLResponse)?.statusCode ?? 0
            guard (200..<300).contains(status) else { throw Failure.response(status) }
            let limit = 8 * 1024 * 1024
            guard response.expectedContentLength <= limit else { throw Failure.invalid }
            var chunk = Data()
            var total = 0
            for try await byte in bytes {
                total += 1
                guard total <= limit else { throw Failure.invalid }
                chunk.append(byte)
                if chunk.count == 2048 {
                    try await player.append(chunk)
                    chunk.removeAll(keepingCapacity: true)
                }
            }
            if !chunk.isEmpty { try await player.append(chunk) }
        } onCancel: {
            session.invalidateAndCancel()
        }
    }

    private func append(_ data: Data) async throws {
        try Task.checkCancellation()
        guard !stopped else { throw CancellationError() }
        try mp3.parse(data)
        guard !mp3.packets.isEmpty else { return }
        if queue == nil {
            try activateOutput()
            try check(AudioQueueNewOutputWithDispatchQueue(&queue, &mp3.format, 0, Self.callbacks) { @Sendable [weak self] _, buffer in
                let address = UInt(bitPattern: buffer)
                Task { @MainActor in
                    guard let self, !self.stopped, let buffer = AudioQueueBufferRef(bitPattern: address) else { return }
                    self.returnedBuffers += 1
                    self.buffers.append(buffer)
                }
            })
        }
        guard let queue else { throw Failure.invalid }
        while buffers.isEmpty && allocated >= 6 {
            try await Task.sleep(for: .milliseconds(5))
            guard !stopped else { throw CancellationError() }
        }
        let buffer: AudioQueueBufferRef
        if let free = buffers.popLast() {
            buffer = free
        } else {
            var new: AudioQueueBufferRef?
            try check(AudioQueueAllocateBuffer(queue, 32768, &new))
            guard let new else { throw Failure.invalid }
            allocated += 1
            buffer = new
        }
        guard mp3.packets.count <= buffer.pointee.mAudioDataBytesCapacity else { throw Failure.invalid }
        mp3.packets.copyBytes(to: buffer.pointee.mAudioData.assumingMemoryBound(to: UInt8.self), count: mp3.packets.count)
        buffer.pointee.mAudioDataByteSize = UInt32(mp3.packets.count)
        try mp3.descriptions.withUnsafeBufferPointer {
            try check(AudioQueueEnqueueBuffer(queue, buffer, UInt32($0.count), $0.baseAddress))
        }
        if !started {
            try check(AudioQueueSetParameter(queue, kAudioQueueParam_Volume, volume))
            try check(AudioQueueStart(queue, nil))
            started = true
        }
    }

    public func stop() {
        stopped = true
        connection?.invalidateAndCancel()
        connection = nil
        if let queue {
            AudioQueueStop(queue, true)
            AudioQueueDispose(queue, true)
            self.queue = nil
        }
        mp3.close()
        buffers.removeAll()
    }

    private func check(_ status: OSStatus) throws {
        if status != noErr { throw Failure.audio(status) }
    }
}

/// AudioFileStream invokes its callbacks synchronously on the thread calling
/// parse. This context never leaves the player or owns an asynchronous callback.
private final class MP3Packets {
    var parser: AudioFileStreamID?
    var format = AudioStreamBasicDescription()
    var packets = Data()
    var descriptions: [AudioStreamPacketDescription] = []
    var invalidPacket = false

    func open() throws {
        let status = AudioFileStreamOpen(Unmanaged.passUnretained(self).toOpaque(), speechProperty,
                                         speechPackets, kAudioFileMP3Type, &parser)
        guard status == noErr else { throw StreamingSpeech.Failure.audio(status) }
    }

    func parse(_ data: Data) throws {
        guard let parser else { throw StreamingSpeech.Failure.invalid }
        packets.removeAll(keepingCapacity: true)
        descriptions.removeAll(keepingCapacity: true)
        let status = data.withUnsafeBytes { bytes -> OSStatus in
            guard let address = bytes.baseAddress else { return noErr }
            return AudioFileStreamParseBytes(parser, UInt32(bytes.count), address, [])
        }
        guard status == noErr else { throw StreamingSpeech.Failure.audio(status) }
        guard !invalidPacket else { throw StreamingSpeech.Failure.invalid }
    }

    func close() {
        if let parser { AudioFileStreamClose(parser); self.parser = nil }
    }

    func readFormat(_ parser: AudioFileStreamID) {
        var size = UInt32(MemoryLayout.size(ofValue: format))
        if AudioFileStreamGetProperty(parser, kAudioFileStreamProperty_DataFormat, &size, &format) != noErr {
            invalidPacket = true
        }
    }

    func collect(_ data: UnsafeRawPointer, bytes: UInt32, count: UInt32,
                             descriptions incoming: UnsafeMutablePointer<AudioStreamPacketDescription>?) {
        guard bytes <= 32768 - packets.count, count > 0, count <= 1024 else { invalidPacket = true; return }
        for index in 0..<Int(count) {
            var entry: AudioStreamPacketDescription
            if let incoming {
                entry = incoming[index]
            } else {
                // Core Audio may omit descriptions for constant-size packets.
                let size = format.mBytesPerPacket
                guard size > 0, UInt64(size) * UInt64(count) == UInt64(bytes) else { invalidPacket = true; return }
                entry = AudioStreamPacketDescription(mStartOffset: Int64(index) * Int64(size),
                                                     mVariableFramesInPacket: 0, mDataByteSize: size)
            }
            guard entry.mStartOffset >= 0, entry.mStartOffset <= Int64(bytes) - Int64(entry.mDataByteSize)
            else { invalidPacket = true; return }
            entry.mStartOffset += Int64(packets.count)
            descriptions.append(entry)
        }
        packets.append(data.assumingMemoryBound(to: UInt8.self), count: Int(bytes))
    }

}

private let speechProperty: AudioFileStream_PropertyListenerProc = { context, parser, id, _ in
    if id == kAudioFileStreamProperty_DataFormat {
        Unmanaged<MP3Packets>.fromOpaque(context).takeUnretainedValue().readFormat(parser)
    }
}
private let speechPackets: AudioFileStream_PacketsProc = { context, bytes, count, data, descriptions in
    Unmanaged<MP3Packets>.fromOpaque(context).takeUnretainedValue().collect(data, bytes: bytes, count: count, descriptions: descriptions)
}
