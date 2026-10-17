@preconcurrency import AVFoundation
import CoreAudio
import KuduCore
import os

/// The audio of a meeting as two 16 kHz mono streams: the microphone ("You") and, when
/// allowed, everything the Mac plays ("Others"). Each stream is cut into pieces at pauses
/// (`SpeechChunker`) and handed out with its start time; audio is kept only until its piece is
/// taken, and never written to disk.
public final class MeetingCapture: @unchecked Sendable {
    public struct Piece: Sendable {
        public var source: AudioSource
        /// Seconds from the start of the meeting.
        public var start: Double
        public var samples: [Float]

        public var end: Double { start + Double(samples.count) / AudioRecorder.sampleRate }
    }

    /// Whether the computer's sound is being recorded (it may have been unavailable).
    public private(set) var hasSystemAudio = false

    private let mic = MicStream()
    private var system: AnyObject?
    private let buffers = OSAllocatedUnfairLock(initialState: [AudioSource: SourceBuffer]())
    private let chunker = SpeechChunker(rate: Int(AudioRecorder.sampleRate))
    private let log = Logger(subsystem: "app.kudu", category: "meeting-audio")
    private var started = ContinuousClock.now

    private struct SourceBuffer {
        /// Samples received so far (or silence filled in), including those already handed out.
        var received = 0
        /// Waiting to be cut into a piece; starts at sample `received - samples.count`.
        var samples: [Float] = []
        /// Microphone audio is replaced with silence while the user dictates.
        var muted = false
        /// Audio actually delivered (not filled in), and its loudest sample.
        var delivered = 0
        var peak: Float = 0
    }

    public init() {}

    /// Opens the microphone and, if `includeSystemAudio`, a tap on the Mac's sound output.
    /// Fails only if the microphone can't be opened; without the tap it records the
    /// microphone alone. (`includeMicrophone: false` is for testing the tap by itself.)
    public func start(includeSystemAudio: Bool, microphoneUID: String?, includeMicrophone: Bool = true) throws {
        buffers.withLock { $0 = [:] }
        started = .now
        if includeMicrophone {
            buffers.withLock { $0[.mic] = SourceBuffer() }
            try mic.start(deviceUID: microphoneUID) { [weak self] samples in self?.receive(samples, from: .mic) }
        }
        guard includeSystemAudio else { return }
        if #available(macOS 14.2, *) {
            let tap = SystemAudioTap()
            do {
                buffers.withLock { $0[.system] = SourceBuffer() }
                try tap.start { [weak self] samples in self?.receive(samples, from: .system) }
                system = tap
                hasSystemAudio = true
            } catch {
                buffers.withLock { $0[.system] = nil }
                log.error("Couldn't record the Mac's sound, using the microphone only: \(error.localizedDescription, privacy: .public)")
            }
        }
    }

    public func stop() {
        mic.stop()
        if #available(macOS 14.2, *) { (system as? SystemAudioTap)?.stop() }
        system = nil
    }

    /// Silences the microphone (while the user dictates into another app).
    public func setMicMuted(_ muted: Bool) {
        buffers.withLock { $0[.mic]?.muted = muted }
    }

    /// The pieces ready to transcribe: each stream's audio up to its next pause. With
    /// `final`, everything that's left.
    public func takePieces(final: Bool = false) -> [Piece] {
        buffers.withLock { buffers in
            var pieces: [Piece] = []
            for source in AudioSource.allCases {
                guard var buffer = buffers[source] else { continue }
                while !buffer.samples.isEmpty {
                    let cut = final ? buffer.samples.count : chunker.nextCut(buffer.samples)
                    guard let cut else { break }
                    let startIndex = buffer.received - buffer.samples.count
                    pieces.append(Piece(source: source, start: Double(startIndex) / AudioRecorder.sampleRate,
                                        samples: Array(buffer.samples[..<cut])))
                    buffer.samples.removeFirst(cut)
                }
                buffers[source] = buffer
            }
            return pieces
        }
    }

    /// How far the computer's sound has been listened to with nothing left to transcribe: the
    /// end of its buffer when what's waiting there is only quiet. Nil if something is waiting.
    public func systemQuietUntil() -> Double? {
        buffers.withLock { buffers in
            guard let buffer = buffers[.system] else { return nil }
            let start = Double(buffer.received - buffer.samples.count) / AudioRecorder.sampleRate
            guard SpeechChunker.hasSpeech(buffer.samples) else { return Double(buffer.received) / AudioRecorder.sampleRate }
            return start
        }
    }

    /// macOS gives a blocked tap exact silence: if a long stretch of the Mac's sound was
    /// nothing but zeros, Kudu most likely isn't allowed to record it.
    public var systemAudioSeemsBlocked: Bool {
        buffers.withLock { buffers in
            guard let buffer = buffers[.system] else { return false }
            return buffer.delivered > Int(AudioRecorder.sampleRate) * 20 && buffer.peak == 0
        }
    }

    private func receive(_ samples: [Float], from source: AudioSource) {
        // The Mac's sound arrives only while something is playing. Fill the gaps with silence,
        // so each stream's sample count stays its clock and "You" and "Others" line up.
        let elapsed = (ContinuousClock.now - started) / .seconds(1)
        let expected = Int(elapsed * AudioRecorder.sampleRate) - samples.count
        buffers.withLock { buffers in
            guard var buffer = buffers[source] else { return }
            let gap = expected - buffer.received
            if gap > Int(AudioRecorder.sampleRate / 4) {
                buffer.samples.append(contentsOf: repeatElement(0, count: gap))
                buffer.received += gap
            }
            buffer.received += samples.count
            buffer.delivered += samples.count
            for sample in samples where abs(sample) > buffer.peak { buffer.peak = abs(sample) }
            if buffer.muted {
                buffer.samples.append(contentsOf: repeatElement(0, count: samples.count))
            } else {
                buffer.samples.append(contentsOf: samples)
            }
            buffers[source] = buffer
        }
    }
}

// MARK: - Microphone

/// The microphone for a meeting, on its own audio engine (dictation keeps using its own, so
/// the user can still dictate during a meeting).
final class MicStream: @unchecked Sendable {
    private let engine = AVAudioEngine()
    private var converter: AVAudioConverter?
    private let format = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: AudioRecorder.sampleRate, channels: 1, interleaved: false)!
    private var onSamples: (([Float]) -> Void)?
    private var deviceUID: String?
    private var observer: NSObjectProtocol?
    private let log = Logger(subsystem: "app.kudu", category: "meeting-audio")

    func start(deviceUID: String?, onSamples: @escaping ([Float]) -> Void) throws {
        self.onSamples = onSamples
        self.deviceUID = deviceUID
        try open()
        observer = NotificationCenter.default.addObserver(forName: .AVAudioEngineConfigurationChange, object: engine, queue: nil) { [weak self] _ in
            // A headset was plugged in or AirPods connected: follow the new hardware.
            guard let self else { return }
            self.log.info("Meeting microphone changed; reopening")
            self.engine.stop()
            self.engine.inputNode.removeTap(onBus: 0)
            try? self.open()
        }
    }

    func stop() {
        if let observer { NotificationCenter.default.removeObserver(observer) }
        observer = nil
        engine.stop()
        engine.inputNode.removeTap(onBus: 0)
        onSamples = nil
    }

    private func open() throws {
        let input = engine.inputNode
        if let deviceUID, let device = AudioDevices.device(uid: deviceUID), let unit = input.audioUnit {
            var id = device.deviceID
            AudioUnitSetProperty(unit, kAudioOutputUnitProperty_CurrentDevice, kAudioUnitScope_Global, 0, &id, UInt32(MemoryLayout<AudioDeviceID>.size))
        }
        let inputFormat = input.outputFormat(forBus: 0)
        guard inputFormat.sampleRate > 0, inputFormat.channelCount > 0 else { throw MeetingAudioError.noMicrophone }
        converter = AVAudioConverter(from: inputFormat, to: format)
        input.installTap(onBus: 0, bufferSize: AVAudioFrameCount(inputFormat.sampleRate * 0.1), format: inputFormat) { [weak self] buffer, _ in
            guard let self, let converter = self.converter,
                  let samples = AudioConversion.convert(buffer, with: converter, to: self.format) else { return }
            self.onSamples?(samples)
        }
        engine.prepare()
        try engine.start()
    }
}

enum MeetingAudioError: Error, LocalizedError {
    case noMicrophone
    case tap(String, OSStatus)

    var errorDescription: String? {
        switch self {
        case .noMicrophone: "No microphone is available"
        case let .tap(step, status): "\(step) failed (\(status))"
        }
    }
}

enum AudioConversion {
    /// Converts one captured buffer to 16 kHz mono.
    static func convert(_ buffer: AVAudioPCMBuffer, with converter: AVAudioConverter, to format: AVAudioFormat) -> [Float]? {
        let ratio = format.sampleRate / buffer.format.sampleRate
        let capacity = AVAudioFrameCount(Double(buffer.frameLength) * ratio) + 32
        guard let output = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: capacity) else { return nil }
        var consumed = false
        var error: NSError?
        converter.convert(to: output, error: &error) { _, status in
            if consumed {
                status.pointee = .noDataNow
                return nil
            }
            consumed = true
            status.pointee = .haveData
            return buffer
        }
        guard error == nil, let channel = output.floatChannelData?[0] else { return nil }
        return Array(UnsafeBufferPointer(start: channel, count: Int(output.frameLength)))
    }
}

// MARK: - The Mac's sound

/// Everything the Mac plays (the other people in a call), through a Core Audio process tap:
/// a tap on all processes except Kudu, read through a private aggregate device. macOS asks
/// the user once to allow "system audio recording".
@available(macOS 14.2, *)
final class SystemAudioTap: @unchecked Sendable {
    private var tapID = AudioObjectID(kAudioObjectUnknown)
    private var aggregateID = AudioObjectID(kAudioObjectUnknown)
    private var procID: AudioDeviceIOProcID?
    private let queue = DispatchQueue(label: "app.kudu.meeting-tap", qos: .userInitiated)
    private let format = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: AudioRecorder.sampleRate, channels: 1, interleaved: false)!

    func start(onSamples: @escaping @Sendable ([Float]) -> Void) throws {
        let ownProcess = try? CoreAudioProperty.read(
            AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyTranslatePIDToProcessObject,
            qualifier: getpid(), as: AudioObjectID.self)
        let description = CATapDescription(monoGlobalTapButExcludeProcesses: ownProcess.map { [$0] } ?? [])
        description.uuid = UUID()
        description.name = "Kudu meeting notes"
        description.isPrivate = true
        description.muteBehavior = .unmuted

        var tap = AudioObjectID(kAudioObjectUnknown)
        var status = AudioHardwareCreateProcessTap(description, &tap)
        guard status == noErr else { throw MeetingAudioError.tap("Creating the audio tap", status) }
        tapID = tap

        do {
            var streamFormat = try CoreAudioProperty.read(tap, kAudioTapPropertyFormat, as: AudioStreamBasicDescription.self)
            guard let tapFormat = AVAudioFormat(streamDescription: &streamFormat),
                  let converter = AVAudioConverter(from: tapFormat, to: format) else {
                throw MeetingAudioError.tap("Reading the tap's format", -1)
            }
            converter.downmix = true

            let output = try CoreAudioProperty.read(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyDefaultSystemOutputDevice, as: AudioDeviceID.self)
            let outputUID = try CoreAudioProperty.readString(output, kAudioDevicePropertyDeviceUID)
            let settings: [String: Any] = [
                kAudioAggregateDeviceNameKey: "Kudu meeting notes",
                kAudioAggregateDeviceUIDKey: UUID().uuidString,
                kAudioAggregateDeviceMainSubDeviceKey: outputUID,
                kAudioAggregateDeviceIsPrivateKey: true,
                kAudioAggregateDeviceIsStackedKey: false,
                kAudioAggregateDeviceTapAutoStartKey: true,
                kAudioAggregateDeviceSubDeviceListKey: [[kAudioSubDeviceUIDKey: outputUID]],
                kAudioAggregateDeviceTapListKey: [[kAudioSubTapDriftCompensationKey: true, kAudioSubTapUIDKey: description.uuid.uuidString]],
            ]
            var aggregate = AudioObjectID(kAudioObjectUnknown)
            status = AudioHardwareCreateAggregateDevice(settings as CFDictionary, &aggregate)
            guard status == noErr else { throw MeetingAudioError.tap("Creating the aggregate device", status) }
            aggregateID = aggregate

            status = AudioDeviceCreateIOProcIDWithBlock(&procID, aggregate, queue) { _, input, _, _, _ in
                guard let buffer = AVAudioPCMBuffer(pcmFormat: tapFormat, bufferListNoCopy: input, deallocator: nil),
                      let samples = AudioConversion.convert(buffer, with: converter, to: self.format) else { return }
                onSamples(samples)
            }
            guard status == noErr else { throw MeetingAudioError.tap("Reading the aggregate device", status) }
            status = AudioDeviceStart(aggregate, procID)
            guard status == noErr else { throw MeetingAudioError.tap("Starting the aggregate device", status) }
        } catch {
            stop()
            throw error
        }
    }

    func stop() {
        if aggregateID != kAudioObjectUnknown {
            AudioDeviceStop(aggregateID, procID)
            if let procID { AudioDeviceDestroyIOProcID(aggregateID, procID) }
            AudioHardwareDestroyAggregateDevice(aggregateID)
            aggregateID = AudioObjectID(kAudioObjectUnknown)
            procID = nil
        }
        if tapID != kAudioObjectUnknown {
            AudioHardwareDestroyProcessTap(tapID)
            tapID = AudioObjectID(kAudioObjectUnknown)
        }
    }
}

// MARK: - Core Audio properties

enum CoreAudioProperty {
    static func address(_ selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress(mSelector: selector, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
    }

    static func read<T>(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector, as type: T.Type) throws -> T {
        var address = address(selector)
        var size = UInt32(MemoryLayout<T>.size)
        let value = UnsafeMutableRawPointer.allocate(byteCount: MemoryLayout<T>.size, alignment: MemoryLayout<T>.alignment)
        defer { value.deallocate() }
        let status = AudioObjectGetPropertyData(object, &address, 0, nil, &size, value)
        guard status == noErr else { throw MeetingAudioError.tap("Reading property \(selector)", status) }
        return value.load(as: T.self)
    }

    static func read<T, Q>(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector, qualifier: Q, as type: T.Type) throws -> T {
        var address = address(selector)
        var qualifier = qualifier
        var size = UInt32(MemoryLayout<T>.size)
        let value = UnsafeMutableRawPointer.allocate(byteCount: MemoryLayout<T>.size, alignment: MemoryLayout<T>.alignment)
        defer { value.deallocate() }
        let status = AudioObjectGetPropertyData(object, &address, UInt32(MemoryLayout<Q>.size), &qualifier, &size, value)
        guard status == noErr else { throw MeetingAudioError.tap("Reading property \(selector)", status) }
        return value.load(as: T.self)
    }

    static func readString(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector) throws -> String {
        var address = address(selector)
        var size = UInt32(MemoryLayout<CFString?>.size)
        var value: Unmanaged<CFString>?
        let status = AudioObjectGetPropertyData(object, &address, 0, nil, &size, &value)
        guard status == noErr, let value else { throw MeetingAudioError.tap("Reading property \(selector)", status) }
        return value.takeRetainedValue() as String
    }

    static func readArray(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector) throws -> [AudioObjectID] {
        var address = address(selector)
        var size: UInt32 = 0
        var status = AudioObjectGetPropertyDataSize(object, &address, 0, nil, &size)
        guard status == noErr else { throw MeetingAudioError.tap("Reading property size \(selector)", status) }
        var ids = [AudioObjectID](repeating: 0, count: Int(size) / MemoryLayout<AudioObjectID>.size)
        status = AudioObjectGetPropertyData(object, &address, 0, nil, &size, &ids)
        guard status == noErr else { throw MeetingAudioError.tap("Reading property \(selector)", status) }
        return ids
    }
}
