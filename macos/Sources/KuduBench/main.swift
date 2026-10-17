// Latency/accuracy harness: transcribes WAV files through the same engine and text
// pipeline the app uses, and prints per-file timings.
//
//   swift run -c release kudu-bench clip1.wav clip2.wav
//   swift run -c release kudu-bench --format clip1.wav          (also run smart formatting)
//   swift run -c release kudu-bench --format --text "raw words"  (format text, no audio)
//   swift run -c release kudu-bench --command "instruction" ["selected text"]   (Command mode)
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
if arguments.first == "--command", arguments.count >= 2 {
    let engine = await CommandEngine()
    let start = ContinuousClock.now
    do {
        let output = try await engine.run(
            instruction: arguments[1], selection: arguments.count > 2 ? arguments[2] : nil, category: .general)
        print(String(format: "[%.0f ms] ", (ContinuousClock.now - start) / .milliseconds(1)) + output)
    } catch {
        print("error: \(error.localizedDescription)")
    }
    exit(0)
}
if arguments.first == "--capture-system", arguments.count >= 2, let seconds = Double(arguments[1]) {
    // Records the Mac's sound for a while through the meeting tap, and transcribes it.
    let engine = ParakeetEngine()
    try await engine.load(SpeechModel.allCases.first { $0.isDownloaded } ?? .base)
    let capture = MeetingCapture()
    try capture.start(includeSystemAudio: true, microphoneUID: nil, includeMicrophone: false)
    print("System audio tap: \(capture.hasSystemAudio)")
    try await Task.sleep(for: .seconds(seconds))
    capture.stop()
    for piece in capture.takePieces(final: true) {
        let peak = piece.samples.map(abs).max() ?? 0
        let text = SpeechChunker.hasSpeech(piece.samples) ? try await engine.transcribe(piece.samples) : "(quiet)"
        print(String(format: "[%.1f–%.1f s, peak %.3f] ", piece.start, piece.end, peak) + text)
    }
    exit(0)
}
if arguments.first == "--meeting", arguments.count >= 2 {
    // A pretend meeting: each clip is one turn, alternating between the other people and
    // you, with a pause between turns. Runs the meeting path: pause cuts, transcription,
    // and the notes.
    let engine = ParakeetEngine()
    try await engine.load(SpeechModel.allCases.first { $0.isDownloaded } ?? .base)
    var audio: [(AudioSource, [Float])] = []
    for (index, path) in arguments.dropFirst().enumerated() {
        audio.append((index % 2 == 0 ? .system : .mic, try loadSamples(URL(fileURLWithPath: path))))
    }
    var segments: [MeetingSegment] = []
    var clock = 0.0
    let chunker = SpeechChunker()
    for (source, samples) in audio {
        var rest = samples + [Float](repeating: 0, count: 16_000)
        var offset = clock
        while !rest.isEmpty {
            let cut = chunker.nextCut(rest) ?? rest.count
            let piece = Array(rest[..<cut])
            rest.removeFirst(cut)
            if SpeechChunker.hasSpeech(piece) {
                let text = try await engine.transcribe(piece)
                segments.append(MeetingSegment(start: offset, end: offset + Double(piece.count) / 16_000, source: source, text: text))
            }
            offset += Double(piece.count) / 16_000
        }
        clock = offset
    }
    print(MeetingFormat.transcriptText(segments, labelled: true))
    let writer = await MeetingNotesWriter()
    print("\nApple Intelligence: \(await writer.usesModel)")
    let start = ContinuousClock.now
    let (title, notes) = await writer.write(segments, heardOthers: true) { done, total in print("  step \(done)/\(total)") }
    print(String(format: "Notes in %.1f s (%@)", (ContinuousClock.now - start) / .seconds(1), notes.generator.rawValue))
    print("TITLE: \(title ?? "-")\nSUMMARY: \(notes.summary)")
    for (name, items) in [("KEY POINTS", notes.keyPoints), ("DECISIONS", notes.decisions), ("ACTIONS", notes.actions)] {
        print(name + ":")
        items.forEach { print("  - " + $0) }
    }
    exit(0)
}
let streamMode = arguments.contains("--stream")
let formatting = arguments.contains("--format") || streamMode
let textMode = arguments.contains("--text")
arguments.removeAll { $0 == "--format" || $0 == "--text" || $0 == "--stream" }
/// "--app Slack --category workMessage": shape formatting for that destination.
@MainActor func option(_ name: String) -> String? {
    guard let index = arguments.firstIndex(of: name), index + 1 < arguments.count else { return nil }
    let value = arguments[index + 1]
    arguments.removeSubrange(index...(index + 1))
    return value
}
let appName = option("--app")
let category = option("--category").flatMap(AppCategory.init(rawValue:)) ?? .general
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
    let worthIt = category == .code
        ? FormattingNeed.shouldFormatCode(text, strength: settings.editingStrength)
        : FormattingNeed.shouldFormat(text, strength: settings.editingStrength, category: category)
    let style = settings.styles.style(for: category)
    if formatting, worthIt {
        let start = ContinuousClock.now
        let request = FormatRequest(text: text, category: category, style: style, appName: appName)
        if let formatted = await formatter.format(request) { text = formatted }
        formatMs = (ContinuousClock.now - start) / .milliseconds(1)
    }
    let context = TextContext(category: category, style: style)
    return (TextPipeline.afterFormatting(settings: settings).process(text, context: context), formatMs)
}

if textMode {
    for raw in arguments {
        let result = await clean(raw)
        print("• \(raw)")
        print("  → \(result.text)" + (result.formatMs.map { String(format: "   [%.0f ms]", $0) } ?? "   [rules only]"))
    }
    exit(0)
}

if !SpeechModel.base.isDownloaded {
    print("Downloading Parakeet model…")
    let lastPercent = OSAllocatedUnfairLock(initialState: -1)
    try await SpeechModel.base.download { fraction in
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
try await engine.load(.base)
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
