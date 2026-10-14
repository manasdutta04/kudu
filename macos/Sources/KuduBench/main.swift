// Latency/accuracy harness: transcribes WAV files through the same engine and text
// pipeline the app uses, and prints per-file timings.
//
//   swift run -c release kudu-bench clip1.wav clip2.wav
//   swift run -c release kudu-bench --format clip1.wav          (also run smart formatting)
//   swift run -c release kudu-bench --format --text "raw words"  (format text, no audio)
//   swift run -c release kudu-bench --stream clip.wav             (replay in real time, as if spoken;
//                                                                  measures release → text like the app)
import AVFoundation
import Foundation
import os
import KuduCore
import KuduKit

func loadSamples(_ url: URL) throws -> [Float] {
    let file = try AVAudioFile(forReading: url)
    let target = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 16_000, channels: 1, interleaved: false)!
    let input = AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: AVAudioFrameCount(file.length))!
    try file.read(into: input)
    let converter = AVAudioConverter(from: file.processingFormat, to: target)!
    let capacity = AVAudioFrameCount(Double(input.frameLength) * 16_000 / file.processingFormat.sampleRate) + 1024
    let output = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: capacity)!
    var consumed = false
    var error: NSError?
    converter.convert(to: output, error: &error) { _, status in
        if consumed { status.pointee = .endOfStream; return nil }
        consumed = true
        status.pointee = .haveData
        return input
    }
    if let error { throw error }
    return Array(UnsafeBufferPointer(start: output.floatChannelData![0], count: Int(output.frameLength)))
}

var arguments = Array(CommandLine.arguments.dropFirst())
let streamMode = arguments.contains("--stream")
let formatting = arguments.contains("--format") || streamMode
let textMode = arguments.contains("--text")
arguments.removeAll { $0 == "--format" || $0 == "--text" || $0 == "--stream" }
guard !arguments.isEmpty else {
    print("usage: kudu-bench [--format] <file.wav>...  |  kudu-bench --format --text \"raw words\"...")
    exit(1)
}

let settings = CleanupSettings()
let formatter = await SmartFormatter()
if formatting {
    print("Smart formatting available: \(await formatter.isAvailable)")
    await formatter.prewarm()
}

/// Rule cleanup, then (with --format) the on-device model, then final touches — like the app.
func clean(_ raw: String) async -> (text: String, formatMs: Double?) {
    var text = TextPipeline.beforeFormatting(settings: settings).process(raw)
    var formatMs: Double?
    if formatting, FormattingNeed.shouldFormat(text, strength: settings.editingStrength) {
        let start = ContinuousClock.now
        if let formatted = await formatter.format(FormatRequest(text: text)) { text = formatted }
        formatMs = (ContinuousClock.now - start) / .milliseconds(1)
    }
    return (TextPipeline.afterFormatting(settings: settings).process(text), formatMs)
}

if textMode {
    for raw in arguments {
        let result = await clean(raw)
        print("• \(raw)")
        print("  → \(result.text)" + (result.formatMs.map { String(format: "   [%.0f ms]", $0) } ?? "   [rules only]"))
    }
    exit(0)
}

if !ParakeetEngine.isDownloaded {
    print("Downloading Parakeet model…")
    let lastPercent = OSAllocatedUnfairLock(initialState: -1)
    try await ParakeetEngine.download { fraction in
        let percent = Int(fraction * 100)
        let changed = lastPercent.withLock { last in
            defer { last = max(last, percent) }
            return percent > last
        }
        if changed {
            print("\r  \(percent)%", terminator: "")
            fflush(stdout)
        }
    }
    print()
}

let engine = ParakeetEngine()
let loadStart = ContinuousClock.now
try await engine.load()
print("Model loaded + warmed in \(ContinuousClock.now - loadStart)")

/// Plays samples back at real-time speed, as a microphone would deliver them.
final class RealTimeSource: SampleSource, @unchecked Sendable {
    let samples: [Float]
    let start = ContinuousClock.now
    init(_ samples: [Float]) { self.samples = samples }

    func samples(from index: Int) -> [Float] {
        let elapsed = (ContinuousClock.now - start) / .seconds(1)
        let available = min(samples.count, Int(elapsed * 16_000))
        return index < available ? Array(samples[index..<available]) : []
    }
}

if streamMode {
    for path in arguments {
        let samples = try loadSamples(URL(fileURLWithPath: path))
        let seconds = Double(samples.count) / 16_000
        print(String(format: "%@  %.1fs — replaying in real time…", path, seconds))
        let source = RealTimeSource(samples)
        let session = await StreamingSession(
            recorder: source, engine: engine, formatter: formatter,
            settings: StreamingSession.Settings(
                before: TextPipeline.beforeFormatting(settings: settings),
                request: FormatRequest(text: ""),
                formattingAvailable: await formatter.isAvailable
            )
        )
        await session.start()
        try await Task.sleep(for: .seconds(seconds))
        let release = ContinuousClock.now
        let result = try await session.finish(samples: samples)
        let text = TextPipeline.afterFormatting(settings: settings).process(result.text)
        let ms = (ContinuousClock.now - release) / .milliseconds(1)
        for line in await session.trace { print("    " + line) }
        print(String(format: "  release → text: %.0f ms   (formatted: %@)", ms, result.formatted ? "yes" : "no"))
        print("  out → \(text)")
    }
    exit(0)
}

for path in arguments {
    let samples = try loadSamples(URL(fileURLWithPath: path))
    let start = ContinuousClock.now
    let raw = try await engine.transcribe(samples)
    let asrMs = (ContinuousClock.now - start) / .milliseconds(1)
    let result = await clean(raw)
    let totalMs = (ContinuousClock.now - start) / .milliseconds(1)
    let seconds = Double(samples.count) / 16_000
    print(String(format: "%@  %.1fs audio  speech %.0f ms (%.0fx real time)  total %.0f ms",
                 path, seconds, asrMs, seconds * 1000 / asrMs, totalMs))
    print("  raw → \(raw)")
    print("  out → \(result.text)")
}
