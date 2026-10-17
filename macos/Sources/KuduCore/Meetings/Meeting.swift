import Foundation

/// Where a piece of meeting audio came from.
public enum AudioSource: String, Codable, Sendable, CaseIterable {
    /// The user's microphone: "You".
    case mic
    /// What the computer plays (the other people in the call): "Others".
    case system

    public var speaker: String {
        switch self {
        case .mic: "You"
        case .system: "Others"
        }
    }
}

/// One transcribed stretch of a meeting. Times are seconds from the start of the meeting.
public struct MeetingSegment: Codable, Sendable, Equatable, Identifiable {
    public var start: Double
    public var end: Double
    public var source: AudioSource
    public var text: String

    public init(start: Double, end: Double, source: AudioSource, text: String) {
        self.start = start
        self.end = end
        self.source = source
        self.text = text
    }

    public var id: String { "\(source.rawValue)-\(start)" }
}

public enum MeetingStatus: String, Codable, Sendable {
    /// Listening and transcribing.
    case recording
    /// The meeting ended; notes are being written.
    case writing
    case done
}

/// What wrote a meeting's notes.
public enum NotesGenerator: String, Codable, Sendable {
    /// Apple Intelligence (Mac).
    case appleIntelligence = "apple-intelligence"
    /// The pack's Kudu writer model (Windows and Linux).
    case kuduWriter = "kudu-writer"
    /// Kudu's built-in rules, when no language model is available.
    case extractive
}

/// The written-up notes for a meeting.
public struct MeetingNotes: Codable, Sendable, Equatable {
    public var summary: String
    public var keyPoints: [String]
    public var decisions: [String]
    public var actions: [String]
    public var generator: NotesGenerator

    public init(summary: String, keyPoints: [String] = [], decisions: [String] = [], actions: [String] = [], generator: NotesGenerator) {
        self.summary = summary
        self.keyPoints = keyPoints
        self.decisions = decisions
        self.actions = actions
        self.generator = generator
    }

    /// Words across every section, for checking a model's notes aren't just the transcript.
    public var wordCount: Int {
        ([summary] + keyPoints + decisions + actions).map(WordCount.count).reduce(0, +)
    }

    public var isEmpty: Bool {
        summary.isEmpty && keyPoints.isEmpty && decisions.isEmpty && actions.isEmpty
    }
}

/// A recorded meeting, saved as `Meetings/<id>/meeting.json`; its transcript is in
/// `transcript.jsonl` next to it.
public struct Meeting: Codable, Sendable, Equatable, Identifiable {
    public var id: UUID
    public var title: String
    public var started: Date
    public var ended: Date?
    /// The meeting app, when Kudu knows it ("Zoom", "Google Chrome").
    public var app: String?
    /// The audio that was recorded: the microphone, and the computer's sound when allowed.
    public var sources: [AudioSource]
    public var status: MeetingStatus
    public var notes: MeetingNotes?
    public var words: Int

    public init(id: UUID = UUID(), title: String, started: Date = .now, app: String?, sources: [AudioSource]) {
        self.id = id
        self.title = title
        self.started = started
        self.ended = nil
        self.app = app
        self.sources = sources
        self.status = .recording
        self.notes = nil
        self.words = 0
    }

    /// Length in seconds (up to now while it's still recording).
    public func duration(now: Date = .now) -> TimeInterval {
        max(0, (ended ?? now).timeIntervalSince(started))
    }

    public var heardOthers: Bool { sources.contains(.system) }

    /// "Meeting in Zoom" or "Meeting", until notes give it a better title.
    public static func defaultTitle(app: String?) -> String {
        app.map { "Meeting in \($0)" } ?? "Meeting"
    }
}

public enum MeetingFormat {
    /// "4:05" or "1:02:09".
    public static func clock(_ seconds: Double) -> String {
        let total = Int(seconds.rounded(.down))
        let hours = total / 3600
        let minutes = (total % 3600) / 60
        let secs = total % 60
        return hours > 0
            ? String(format: "%d:%02d:%02d", hours, minutes, secs)
            : String(format: "%d:%02d", minutes, secs)
    }

    /// "34 min", "1 h 5 min", "under a minute".
    public static func length(_ seconds: Double) -> String {
        let minutes = Int((seconds / 60).rounded())
        if seconds < 60 { return "under a minute" }
        if minutes < 60 { return "\(minutes) min" }
        let rest = minutes % 60
        return rest == 0 ? "\(minutes / 60) h" : "\(minutes / 60) h \(rest) min"
    }

    /// Consecutive segments from the same speaker joined into one turn, in time order.
    public static func turns(_ segments: [MeetingSegment]) -> [MeetingSegment] {
        var turns: [MeetingSegment] = []
        for segment in segments.sorted(by: { $0.start < $1.start }) {
            let text = segment.text.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !text.isEmpty else { continue }
            if var last = turns.last, last.source == segment.source, segment.start - last.end < 30 {
                last.text += " " + text
                last.end = segment.end
                turns[turns.count - 1] = last
            } else {
                turns.append(MeetingSegment(start: segment.start, end: segment.end, source: segment.source, text: text))
            }
        }
        return turns
    }

    /// The transcript as plain text for a language model: "You: …" and "Others: …" lines, or
    /// just the text when only the microphone was recorded.
    public static func transcriptText(_ segments: [MeetingSegment], labelled: Bool) -> String {
        turns(segments)
            .map { labelled ? "\($0.source.speaker): \($0.text)" : $0.text }
            .joined(separator: "\n")
    }

    /// The meeting as a Markdown document: notes, then the transcript.
    public static func markdown(_ meeting: Meeting, segments: [MeetingSegment], dateText: String) -> String {
        var lines = ["# \(meeting.title)", ""]
        var meta = [dateText, length(meeting.duration())]
        if let app = meeting.app { meta.append(app) }
        lines += [meta.joined(separator: " · "), ""]
        if let notes = meeting.notes {
            if !notes.summary.isEmpty { lines += ["## Summary", "", notes.summary, ""] }
            for (title, items, prefix) in [("Key points", notes.keyPoints, "- "), ("Decisions", notes.decisions, "- "), ("Action items", notes.actions, "- [ ] ")]
            where !items.isEmpty {
                lines += ["## \(title)", ""] + items.map { prefix + $0 } + [""]
            }
        }
        let labelled = meeting.heardOthers
        lines += ["## Transcript", ""]
        for turn in turns(segments) {
            let stamp = clock(turn.start)
            lines.append(labelled ? "**\(turn.source.speaker)** (\(stamp)): \(turn.text)" : "(\(stamp)) \(turn.text)")
            lines.append("")
        }
        return lines.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines) + "\n"
    }
}
