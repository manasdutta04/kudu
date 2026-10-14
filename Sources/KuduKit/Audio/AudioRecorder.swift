@preconcurrency import AVFoundation
import os

/// Captures microphone audio as 16 kHz mono Float32, the format Parakeet expects.
///
/// Latency notes:
/// - The engine is built and `prepare()`d ahead of time, so `start()` only has to start the
///   audio unit.
/// - With `keepWarm`, the engine keeps running between dictations and the last
///   `preRollSeconds` of audio are kept, so the first syllable is never clipped. This keeps
///   the macOS microphone indicator on, so it's opt-in.
public final class AudioRecorder: @unchecked Sendable {
    public static let sampleRate: Double = 16_000

    /// Called on the audio thread with the RMS level (0...1) of each captured buffer.
    public var onLevel: (@Sendable (Float) -> Void)?

    public var keepWarm = false {
        didSet {
            let warm = keepWarm
            state.withLock { $0.keepWarm = warm }
            warm ? warmUp() : coolDownIfIdle()
        }
    }
    public var preRollSeconds: Double = 0.3

    private let engine = AVAudioEngine()
    private let targetFormat = AVAudioFormat(
        commonFormat: .pcmFormatFloat32, sampleRate: AudioRecorder.sampleRate, channels: 1, interleaved: false
    )!
    private var converter: AVAudioConverter?
    private var tapInstalled = false

    private struct State {
        var isRecording = false
        var samples: [Float] = []
        var preRoll: [Float] = []
        var keepWarm = false
    }
    private let state = OSAllocatedUnfairLock(initialState: State())
    private let log = Logger(subsystem: "app.kudu", category: "audio")
    private var configObserver: NSObjectProtocol?

    public init() {
        configObserver = NotificationCenter.default.addObserver(
            forName: .AVAudioEngineConfigurationChange, object: engine, queue: nil
        ) { [weak self] _ in
            self?.handleConfigurationChange()
        }
    }

    deinit {
        if let configObserver { NotificationCenter.default.removeObserver(configObserver) }
    }

    /// Builds the tap and prepares the engine so the first `start()` is fast.
    public func prepare() {
        installTapIfNeeded()
        engine.prepare()
    }

    public func start() throws {
        state.withLock { state in
            state.isRecording = true
            state.samples = state.preRoll
            state.samples.reserveCapacity(Int(Self.sampleRate) * 30)
        }
        installTapIfNeeded()
        if !engine.isRunning {
            try engine.start()
        }
    }

    /// Stops capturing and returns everything recorded since `start()`.
    public func stop() -> [Float] {
        let samples = state.withLock { state -> [Float] in
            state.isRecording = false
            defer { state.samples = [] }
            return state.samples
        }
        coolDownIfIdle()
        return samples
    }

    public var isRecording: Bool { state.withLock { $0.isRecording } }

    // MARK: - Private

    private func warmUp() {
        installTapIfNeeded()
        if !engine.isRunning { try? engine.start() }
    }

    private func coolDownIfIdle() {
        guard !keepWarm, !isRecording, engine.isRunning else { return }
        engine.pause()
        state.withLock { $0.preRoll = [] }
    }

    private func installTapIfNeeded() {
        guard !tapInstalled else { return }
        let input = engine.inputNode
        let inputFormat = input.outputFormat(forBus: 0)
        guard inputFormat.sampleRate > 0, inputFormat.channelCount > 0 else {
            log.error("No usable input device (format \(inputFormat))")
            return
        }
        converter = AVAudioConverter(from: inputFormat, to: targetFormat)
        // ~20 ms buffers keep the level meter responsive without much overhead.
        let bufferSize = AVAudioFrameCount(inputFormat.sampleRate * 0.02)
        input.installTap(onBus: 0, bufferSize: bufferSize, format: inputFormat) { [weak self] buffer, _ in
            self?.process(buffer)
        }
        tapInstalled = true
    }

    private func process(_ buffer: AVAudioPCMBuffer) {
        guard let converted = convert(buffer), let channel = converted.floatChannelData?[0] else { return }
        let chunk = UnsafeBufferPointer(start: channel, count: Int(converted.frameLength))

        var sum: Float = 0
        for sample in chunk { sum += sample * sample }
        let rms = chunk.isEmpty ? 0 : (sum / Float(chunk.count)).squareRoot()
        onLevel?(min(1, rms * 4))

        let preRollLimit = Int(preRollSeconds * Self.sampleRate)
        let samples = Array(chunk)
        state.withLock { state in
            if state.isRecording {
                state.samples.append(contentsOf: samples)
            } else if state.keepWarm {
                state.preRoll.append(contentsOf: samples)
                if state.preRoll.count > preRollLimit {
                    state.preRoll.removeFirst(state.preRoll.count - preRollLimit)
                }
            }
        }
    }

    private func convert(_ buffer: AVAudioPCMBuffer) -> AVAudioPCMBuffer? {
        guard let converter else { return nil }
        let ratio = targetFormat.sampleRate / buffer.format.sampleRate
        let capacity = AVAudioFrameCount(Double(buffer.frameLength) * ratio) + 32
        guard let output = AVAudioPCMBuffer(pcmFormat: targetFormat, frameCapacity: capacity) else { return nil }
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
        if let error {
            log.error("Audio conversion failed: \(error.localizedDescription)")
            return nil
        }
        return output
    }

    /// The input device changed (headset plugged in, AirPods connected...). Rebuild the tap
    /// for the new hardware format.
    private func handleConfigurationChange() {
        log.info("Audio configuration changed; rebuilding input tap")
        let wasRunning = isRecording || keepWarm
        engine.stop()
        if tapInstalled {
            engine.inputNode.removeTap(onBus: 0)
            tapInstalled = false
        }
        installTapIfNeeded()
        engine.prepare()
        if wasRunning { try? engine.start() }
    }
}
