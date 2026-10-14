// Latency/accuracy harness: transcribes WAV files through the same engine and text
// pipeline the app uses, and prints per-file timings.
//
//   swift run -c release kudu-bench clip1.wav clip2.wav
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

let paths = CommandLine.arguments.dropFirst()
guard !paths.isEmpty else {
    print("usage: kudu-bench <file.wav>...")
    exit(1)
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

for path in paths {
    let samples = try loadSamples(URL(fileURLWithPath: path))
    let start = ContinuousClock.now
    let raw = try await engine.transcribe(samples)
    let text = TextPipeline.basic.process(raw)
    let elapsed = (ContinuousClock.now - start) / .milliseconds(1)
    let seconds = Double(samples.count) / 16_000
    print(String(format: "%@  %.1fs audio  %.0f ms  (%.0fx real time)", path, seconds, elapsed, seconds * 1000 / elapsed))
    print("  → \(text)")
}
