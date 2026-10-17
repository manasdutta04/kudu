import Foundation

/// Puts the two transcripts of a meeting (the microphone and the computer's sound) together,
/// and drops what the microphone heard from the speakers. Without headphones the microphone
/// picks up the other people too, so the same words would otherwise appear twice, once as
/// "You".
///
/// A microphone segment is held back until the computer's sound has been transcribed past it
/// (plus `window` seconds), so it can be compared with what the others said at that time.
public struct TranscriptAssembler: Sendable {
    /// How far apart in time an echo and its source can be, in seconds.
    public var window: Double
    /// The share of a microphone segment's words that must match to call it an echo.
    public var threshold: Double
    /// False when only the microphone is recorded: segments are released at once.
    public var hasSystemAudio: Bool

    private var pending: [MeetingSegment] = []
    private var recentSystem: [MeetingSegment] = []
    private var systemHeardUntil: Double = 0

    public init(hasSystemAudio: Bool, window: Double = 4, threshold: Double = 0.6) {
        self.hasSystemAudio = hasSystemAudio
        self.window = window
        self.threshold = threshold
    }

    /// Adds a transcribed segment; returns the segments that are ready to save, in time order.
    public mutating func add(_ segment: MeetingSegment) -> [MeetingSegment] {
        guard hasSystemAudio else { return [segment] }
        switch segment.source {
        case .system:
            recentSystem.append(segment)
            systemHeardUntil = max(systemHeardUntil, segment.end)
            return ([segment] + release()).sorted { $0.start < $1.start }
        case .mic:
            pending.append(segment)
            return release()
        }
    }

    /// The computer's sound has been listened to up to `time`, even if nothing was said, so
    /// microphone segments before it can be released.
    public mutating func systemHeard(until time: Double) -> [MeetingSegment] {
        guard hasSystemAudio else { return [] }
        systemHeardUntil = max(systemHeardUntil, time)
        return release()
    }

    /// The meeting ended: everything still held back.
    public mutating func finish() -> [MeetingSegment] {
        let rest = pending.filter { !isEcho($0) }
        pending.removeAll()
        return rest
    }

    private mutating func release() -> [MeetingSegment] {
        let due = pending.filter { $0.end + window <= systemHeardUntil }
        guard !due.isEmpty else { return [] }
        pending.removeAll { $0.end + window <= systemHeardUntil }
        let ready = due.filter { !isEcho($0) }
        let oldest = (pending.map(\.start).min() ?? systemHeardUntil) - window * 2
        recentSystem.removeAll { $0.end < oldest }
        return ready
    }

    func isEcho(_ mic: MeetingSegment) -> Bool {
        let heard = recentSystem
            .filter { $0.end >= mic.start - window && $0.start <= mic.end + window }
            .flatMap { Self.words($0.text) }
        return Self.overlap(Self.words(mic.text), Set(heard)) >= threshold
    }

    /// The share of `words` found in `heard`. Very short segments must match completely.
    static func overlap(_ words: [String], _ heard: Set<String>) -> Double {
        guard !words.isEmpty, !heard.isEmpty else { return 0 }
        let found = words.filter(heard.contains).count
        if words.count < 3 { return found == words.count ? 1 : 0 }
        return Double(found) / Double(words.count)
    }

    static func words(_ text: String) -> [String] {
        text.lowercased()
            .split { !$0.isLetter && !$0.isNumber && $0 != "'" }
            .map(String.init)
    }
}
