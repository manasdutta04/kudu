@preconcurrency import AVFoundation
import os

/// Captures microphone audio as 16 kHz mono Float32, the format Parakeet expects.
///
/// The microphone is open only while a dictation is being recorded: `start()` opens it and
/// `stop()` closes it, so the macOS microphone indicator is never on otherwise. The engine is
/// `prepare()`d ahead of time (and again after each stop) so opening it is quick.
public final class AudioRecorder: @unchecked Sendable {
    public static let sampleRate: Double = 16_000

    /// Called on the audio thread with the raw RMS amplitude of each captured buffer (~20 ms).
    public var onLevel: (@Sendable (Float) -> Void)?
    /// Called on the audio thread when the microphone delivers its first audio after `start()`:
    /// the moment the user can start talking without losing a word.
    public var onLive: (@Sendable () -> Void)?


    private let engine = AVAudioEngine()
    private let targetFormat = AVAudioFormat(
        commonFormat: .pcmFormatFloat32, sampleRate: AudioRecorder.sampleRate, channels: 1, interleaved: false
    )!
    private var converter: AVAudioConverter?
    private var tapInstalled = false

    private struct State {
        var isRecording = false
        var samples: [Float] = []
        var startRequestedAt: ContinuousClock.Instant?
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
            state.samples = []
            state.samples.reserveCapacity(Int(Self.sampleRate) * 30)
            // Measure how long the microphone takes to deliver audio (Bluetooth is slow).
            state.startRequestedAt = .now
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
        closeMicrophone()
        return samples
    }

    public var isRecording: Bool { state.withLock { $0.isRecording } }

    /// A copy of what has been recorded so far, from sample `index` on, without stopping.
    public func samples(from index: Int) -> [Float] {
        state.withLock { state in
            guard index < state.samples.count else { return [] }
            return Array(state.samples[index...])
        }
    }

    // MARK: - Private

    /// Stops the audio hardware (the microphone indicator goes off) and prepares the engine
    /// again so the next `start()` is fast.
    private func closeMicrophone() {
        guard engine.isRunning else { return }
        engine.stop()
        engine.prepare()
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
        onLevel?(rms)

        let samples = Array(chunk)
        let startupDelay = state.withLock { state -> Duration? in
            guard state.isRecording else { return nil }
            state.samples.append(contentsOf: samples)
            guard let requested = state.startRequestedAt else { return nil }
            state.startRequestedAt = nil
            return ContinuousClock.now - requested
        }
        if let startupDelay {
            onLive?()
            log.info("Microphone delivered first audio after \(Int(startupDelay / .milliseconds(1))) ms")
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
        let wasRunning = isRecording
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
