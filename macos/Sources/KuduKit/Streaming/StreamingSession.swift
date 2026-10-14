import Foundation
import KuduCore
import os

/// Audio recorded so far, readable while recording continues.
public protocol SampleSource: AnyObject, Sendable {
    func samples(from index: Int) -> [Float]
}

extension AudioRecorder: SampleSource {}

/// Does the slow part of a dictation — formatting — *while the user is still speaking*, so
/// releasing the key leaves almost nothing to do:
///
/// 1. Every ~1.5 s the whole recording so far is re-transcribed. Recognition is cheap (a long
///    dictation takes ~0.2 s on the Neural Engine) and whole-recording passes are the most
///    accurate.
/// 2. The transcript is split into sentence-aligned chunks (`SentenceChunker`). Chunks that are
///    complete are formatted in the background, each as a continuation of the ones before,
///    and cached by their source text.
/// 3. On release, one final full transcription; cached chunks whose text is unchanged are
///    reused, and only the remaining sentences are formatted (or finished by the rules).
@MainActor
public final class StreamingSession {
    public struct Settings {
        public var before: TextPipeline
        public var request: FormatRequest // text/previous are filled in per call
        public var formattingAvailable: Bool

        public init(before: TextPipeline, request: FormatRequest, formattingAvailable: Bool) {
            self.before = before
            self.request = request
            self.formattingAvailable = formattingAvailable
        }
    }

    public struct Result: Sendable {
        public var raw: String
        public var text: String
        public var formatted: Bool
    }

    private struct Piece {
        var source: String
        var text: String
        var usedModel: Bool
    }

    private let recorder: any SampleSource
    private let engine: ParakeetEngine
    private let formatter: SmartFormatter
    private let settings: Settings
    private let log = Logger(subsystem: "app.kudu", category: "streaming")

    /// Formatted chunks, in order, keyed by their raw source text.
    private var pieces: [Piece] = []
    private var inFlight: (source: String, task: Task<Piece, Never>)?
    private var transcribedSamples = 0
    private var loop: Task<Void, Never>?

    /// Diagnostics: one line per event, for kudu-bench.
    public private(set) var trace: [String] = []
    private let born = ContinuousClock.now
    private func note(_ line: String) {
        trace.append(String(format: "%6.2fs  ", (ContinuousClock.now - born) / .seconds(1)) + line)
    }

    public init(recorder: any SampleSource, engine: ParakeetEngine, formatter: SmartFormatter, settings: Settings) {
        self.recorder = recorder
        self.engine = engine
        self.formatter = formatter
        self.settings = settings
    }

    public func start() {
        guard settings.formattingAvailable else { return } // nothing to get ahead on
        loop = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(1500))
                guard !Task.isCancelled, let self else { return }
                await self.refresh()
            }
        }
    }

    public func cancel() {
        loop?.cancel()
        inFlight?.task.cancel()
    }

    /// Called after the recorder stopped, with everything it recorded.
    public func finish(samples: [Float]) async throws -> Result {
        note("release")
        loop?.cancel()
        await loop?.value

        let start = ContinuousClock.now
        let raw = try await engine.transcribe(samples)
        note("final ASR \(Int((ContinuousClock.now - start) / .milliseconds(1))) ms")
        let split = SentenceChunker.chunks(raw)
        let sources = split.stable + (split.rest.isEmpty ? [] : [split.rest])

        // The chunk being formatted when the key was released is usually still valid: let it finish.
        if let inFlight, sources.count > pieces.count, sources[pieces.count] == inFlight.source {
            pieces.append(await inFlight.task.value)
            note("waited for in-flight chunk")
        }
        self.inFlight = nil

        // Keep cached chunks while the transcript still agrees with them.
        var reused = 0
        while reused < pieces.count, reused < sources.count, pieces[reused].source == sources[reused] { reused += 1 }
        pieces.removeSubrange(reused...)
        note("reused \(reused) of \(sources.count) chunks")

        // Whatever is left is formatted as one continuation.
        let remaining = sources.dropFirst(reused).joined(separator: " ")
        if !remaining.isEmpty {
            let started = ContinuousClock.now
            pieces.append(await format(remaining))
            note("formatted remaining \(WordCount.count(remaining)) words in \(Int((ContinuousClock.now - started) / .milliseconds(1))) ms (model: \(pieces.last!.usedModel))")
        }
        let text = pieces.reduce("") { Self.join($0, $1.text) }
        return Result(raw: raw, text: text, formatted: pieces.contains(where: \.usedModel))
    }

    // MARK: - While speaking

    private func refresh() async {
        let samples = recorder.samples(from: 0)
        // Only re-transcribe when there is meaningfully more audio.
        guard samples.count >= transcribedSamples + 16_000 else { return }
        let start = ContinuousClock.now
        guard let raw = try? await engine.transcribe(samples), !Task.isCancelled else { return }
        transcribedSamples = samples.count
        let stable = SentenceChunker.chunks(raw).stable
        note("ASR of \(String(format: "%.1f", Double(samples.count) / 16_000))s in \(Int((ContinuousClock.now - start) / .milliseconds(1))) ms, \(stable.count) stable chunks")

        // Drop cached chunks the new transcript disagrees with.
        var valid = 0
        while valid < pieces.count, valid < stable.count, pieces[valid].source == stable[valid] { valid += 1 }
        if valid < pieces.count { pieces.removeSubrange(valid...) }
        if let inFlight, stable.count <= pieces.count || stable[pieces.count] != inFlight.source {
            inFlight.task.cancel()
            self.inFlight = nil
        }
        // Start the next chunk if nothing is being formatted.
        if let finished = inFlight, finished.source == stable[safe: pieces.count] {
            return // still working on it
        }
        startNext(stable)
    }

    private func startNext(_ stable: [String]) {
        guard inFlight == nil, let source = stable[safe: pieces.count] else { return }
        let task = Task { [weak self] () -> Piece in
            guard let self else { return Piece(source: source, text: source, usedModel: false) }
            return await self.format(source)
        }
        inFlight = (source, task)
        Task { [weak self] in
            let piece = await task.value
            guard let self, !task.isCancelled, self.inFlight?.source == source else { return }
            self.pieces.append(piece)
            self.inFlight = nil
            self.note("chunk formatted in background (model: \(piece.usedModel)): \(piece.text.prefix(60))…")
        }
    }

    /// Formats one chunk: on its own if it's the first, otherwise as a continuation of the
    /// chunks before it. Plain chunks are finished by the rules alone.
    private func format(_ source: String) async -> Piece {
        let clean = settings.before.process(source)
        guard settings.formattingAvailable, FormattingNeed.shouldFormat(clean, strength: settings.request.strength) else {
            return Piece(source: source, text: clean, usedModel: false)
        }
        var request = settings.request
        request.text = clean
        let previous = pieces.reduce("") { Self.join($0, $1.text) }
        request.previous = previous.isEmpty ? nil : previous
        if let result = await formatter.format(request) {
            return Piece(source: source, text: result, usedModel: true)
        }
        return Piece(source: source, text: clean, usedModel: false)
    }

    /// Joins formatted text and its continuation: list items and paragraphs keep their own
    /// line breaks, everything else continues the sentence flow.
    static func join(_ head: String, _ tail: String) -> String {
        let tail = tail.trimmingCharacters(in: .whitespaces)
        if head.isEmpty { return tail }
        if tail.isEmpty { return head }
        if tail.hasPrefix("\n") || head.hasSuffix("\n") { return head + tail }
        // A new numbered item continues a list on its own line.
        if let first = tail.first, first.isNumber, tail.dropFirst().prefix(3).contains("."),
           head.split(separator: "\n").last?.first?.isNumber == true {
            return head + "\n" + tail
        }
        return head + " " + tail
    }
}

private extension Array {
    subscript(safe index: Int) -> Element? { indices.contains(index) ? self[index] : nil }
}
