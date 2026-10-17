import Foundation
import KuduCore
import os
#if canImport(FoundationModels)
import FoundationModels
#endif

/// Writes a meeting's notes with Apple Intelligence: each part of a long transcript is
/// summarised, then the summaries become the notes. Without Apple Intelligence (or if its
/// reply can't be used) the notes come from Kudu's own rules instead.
@MainActor
public final class MeetingNotesWriter {
    private let log = Logger(subsystem: "app.kudu", category: "meetings")

    public init() {}

    /// Whether Apple Intelligence is available to write notes.
    public var usesModel: Bool {
        #if canImport(FoundationModels)
        if #available(macOS 26.0, *), case .available = SystemLanguageModel.default.availability { return true }
        #endif
        return false
    }

    /// The meeting's title (if the model gave one) and notes. `progress` gets (done, total)
    /// steps.
    public func write(_ segments: [MeetingSegment], heardOthers: Bool, progress: @escaping (Int, Int) -> Void) async -> (title: String?, notes: MeetingNotes) {
        let transcript = MeetingFormat.transcriptText(segments, labelled: heardOthers)
        let words = WordCount.count(transcript)
        if words >= 25, usesModel, let written = await withModel(transcript, words: words, progress: progress) {
            return written
        }
        return (nil, MeetingNotesPrompt.extractive(segments))
    }

    private func withModel(_ transcript: String, words: Int, progress: @escaping (Int, Int) -> Void) async -> (title: String?, notes: MeetingNotes)? {
        #if canImport(FoundationModels)
        guard #available(macOS 26.0, *) else { return nil }
        let start = ContinuousClock.now
        let parts = MeetingNotesPrompt.parts(transcript)
        let total = parts.count > 1 ? parts.count + 1 : 1
        progress(0, total)
        let input: String
        if parts.count > 1 {
            var summaries: [String] = []
            for (index, part) in parts.enumerated() {
                let prompt = MeetingNotesPrompt.partPrompt(part, index: index, of: parts.count)
                guard let summary = await respond(MeetingNotesPrompt.partInstructions, prompt) else { return nil }
                summaries.append(summary)
                progress(index + 1, total)
            }
            input = MeetingNotesPrompt.notesPrompt(partSummaries: summaries)
        } else {
            input = MeetingNotesPrompt.notesPrompt(transcript: transcript)
        }
        guard let reply = await respond(MeetingNotesPrompt.notesInstructions, input),
              let parsed = MeetingNotesPrompt.parse(reply, generator: .appleIntelligence),
              MeetingNotesPrompt.isUsable(parsed.notes, transcriptWords: words)
        else {
            log.error("Apple Intelligence's notes weren't usable; using the built-in notes")
            return nil
        }
        progress(total, total)
        log.info("Meeting notes written in \(Int((ContinuousClock.now - start) / .seconds(1))) s from \(words) words (\(parts.count) part(s))")
        return parsed
        #else
        return nil
        #endif
    }

    #if canImport(FoundationModels)
    @available(macOS 26.0, *)
    private func respond(_ instructions: String, _ prompt: String) async -> String? {
        let session = LanguageModelSession(instructions: instructions)
        do {
            return try await withThrowingTaskGroup(of: String?.self) { group in
                group.addTask { try await session.respond(to: prompt, options: GenerationOptions(sampling: .greedy)).content }
                group.addTask {
                    try await Task.sleep(for: .seconds(60))
                    return nil
                }
                let first = try await group.next() ?? nil
                group.cancelAll()
                return first
            }
        } catch {
            log.error("Apple Intelligence couldn't write notes: \(error.localizedDescription, privacy: .public)")
            return nil
        }
    }
    #endif
}
