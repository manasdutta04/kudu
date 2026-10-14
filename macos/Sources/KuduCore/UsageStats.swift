import Foundation

/// One finished dictation, as kept in the on-device history.
public struct DictationRecord: Codable, Identifiable, Sendable, Equatable {
    public var id: UUID
    public var date: Date
    /// Exactly what the speech model returned, before cleanup.
    public var raw: String
    /// What was inserted.
    public var text: String
    public var words: Int
    /// Length of the recording.
    public var audioSeconds: Double
    /// Key release → text inserted.
    public var latencyMs: Int
    /// The app the text went to.
    public var bundleID: String?
    /// Whether smart formatting (the on-device language model) ran.
    public var formatted: Bool
    /// A Command mode result (an edit or draft) rather than a dictation. Nil in older records.
    public var command: Bool?
    /// Words changed by cleanup between the raw transcript and the final text (fillers removed,
    /// corrections, dictionary spellings). Nil in older records.
    public var cleanups: Int?
    /// Dictionary spellings applied. Nil in older records.
    public var dictionaryFixes: Int?

    public init(
        id: UUID = UUID(), date: Date = .now, raw: String, text: String, audioSeconds: Double,
        latencyMs: Int, bundleID: String? = nil, formatted: Bool = false, command: Bool = false,
        dictionaryFixes: Int = 0
    ) {
        self.id = id
        self.date = date
        self.raw = raw
        self.text = text
        self.words = WordCount.count(text)
        self.audioSeconds = audioSeconds
        self.latencyMs = latencyMs
        self.bundleID = bundleID
        self.formatted = formatted
        self.command = command ? true : nil
        self.cleanups = command ? 0 : FormattingGuard.editDistance(FormattingGuard.words(raw), FormattingGuard.words(text))
        self.dictionaryFixes = dictionaryFixes
    }

    public var isCommand: Bool { command == true }
}

public enum WordCount {
    /// Words as people count them: whitespace-separated tokens containing a letter or digit
    /// ("—" and "-" alone don't count).
    public static func count(_ text: String) -> Int {
        text.split(whereSeparator: \.isWhitespace)
            .filter { $0.contains(where: { $0.isLetter || $0.isNumber }) }
            .count
    }
}

/// Headline numbers for the home screen.
public struct UsageStats: Equatable, Sendable {
    public var totalWords: Int
    /// Speaking speed across dictations long enough to measure (≥ 1.5 s). 0 when unknown.
    public var wordsPerMinute: Int
    /// Consecutive days with at least one dictation, ending today (or yesterday, so the
    /// streak doesn't read 0 first thing in the morning).
    public var dayStreak: Int

    public init(totalWords: Int, wordsPerMinute: Int, dayStreak: Int) {
        self.totalWords = totalWords
        self.wordsPerMinute = wordsPerMinute
        self.dayStreak = dayStreak
    }

    public static let empty = UsageStats(totalWords: 0, wordsPerMinute: 0, dayStreak: 0)

    public static func compute(
        _ records: [DictationRecord], now: Date = .now, calendar: Calendar = .current
    ) -> UsageStats {
        let totalWords = records.reduce(0) { $0 + $1.words }

        let timed = records.filter { $0.audioSeconds >= 1.5 }
        let minutes = timed.reduce(0) { $0 + $1.audioSeconds } / 60
        let timedWords = timed.reduce(0) { $0 + $1.words }
        let wordsPerMinute = minutes > 0 ? Int((Double(timedWords) / minutes).rounded()) : 0

        let days = Set(records.map { calendar.startOfDay(for: $0.date) })
        var day = calendar.startOfDay(for: now)
        if !days.contains(day), let yesterday = calendar.date(byAdding: .day, value: -1, to: day) {
            day = yesterday
        }
        var streak = 0
        while days.contains(day) {
            streak += 1
            guard let previous = calendar.date(byAdding: .day, value: -1, to: day) else { break }
            day = previous
        }

        return UsageStats(totalWords: totalWords, wordsPerMinute: wordsPerMinute, dayStreak: streak)
    }
}
