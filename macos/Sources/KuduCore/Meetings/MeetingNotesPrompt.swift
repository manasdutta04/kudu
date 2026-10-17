import Foundation

/// How meeting notes are written: the prompts for an on-device language model, reading its
/// reply, and plain rules for when no model is available.
///
/// A long meeting doesn't fit in a small model's context, so the transcript is split into
/// parts; each part is summarised, then the summaries are combined into the notes.
public enum MeetingNotesPrompt {
    /// Words per part: about 2,000 tokens, leaving room for the instructions and the reply in
    /// a 4,096-token context.
    public static let partWords = 1_400

    public static let partInstructions = """
        You summarise one part of a meeting transcript. Lines start with who spoke: "You" is \
        the person taking notes, "Others" is everyone else.
        Write short bullet points ("- ") covering what was discussed, anything decided, and any \
        task someone agreed to do (with who and when, if said). Use only what is in the \
        transcript. Never invent names, numbers or dates. Output only the bullet points.
        """

    public static let notesInstructions = """
        You write meeting notes from a transcript or from summaries of its parts. Lines may \
        start with who spoke: "You" is the person taking notes, "Others" is everyone else.
        Use only what is in the input. Never invent names, numbers or dates. Be brief and \
        concrete. Write in this exact format, and write None under a heading with nothing:
        TITLE: a short title for the meeting, at most 8 words
        SUMMARY: two to four sentences
        KEY POINTS:
        - one point per line
        DECISIONS:
        - one decision per line
        ACTION ITEMS:
        - who will do what, and by when if said
        """

    /// The transcript split into parts of about `partWords` words, at line or sentence ends.
    public static func parts(_ transcript: String, maxWords: Int = partWords) -> [String] {
        var parts: [String] = []
        var current: [String] = []
        var words = 0
        let pieces = transcript.split(separator: "\n").flatMap { line -> [String] in
            let line = String(line)
            return WordCount.count(line) > maxWords ? SentenceChunker.sentences(line) : [line]
        }
        for piece in pieces {
            let count = WordCount.count(piece)
            if words + count > maxWords, !current.isEmpty {
                parts.append(current.joined(separator: "\n"))
                current = []
                words = 0
            }
            current.append(piece)
            words += count
        }
        if !current.isEmpty { parts.append(current.joined(separator: "\n")) }
        return parts
    }

    public static func partPrompt(_ part: String, index: Int, of count: Int) -> String {
        "Part \(index + 1) of \(count) of the meeting transcript:\n\n\(part)"
    }

    public static func notesPrompt(transcript: String) -> String {
        "Meeting transcript:\n\n\(transcript)"
    }

    public static func notesPrompt(partSummaries: [String]) -> String {
        let joined = partSummaries.enumerated()
            .map { "Part \($0.offset + 1):\n\($0.element)" }
            .joined(separator: "\n\n")
        return "Summaries of the meeting, part by part:\n\n\(joined)"
    }

    /// Reads a model's reply in the notes format. Nil if it isn't in that format.
    public static func parse(_ reply: String, generator: NotesGenerator) -> (title: String?, notes: MeetingNotes)? {
        var title: String?
        var summary: [String] = []
        var lists: [Heading: [String]] = [:]
        var current: Heading?
        var headings = 0

        for rawLine in reply.split(separator: "\n", omittingEmptySubsequences: true) {
            var line = rawLine.trimmingCharacters(in: .whitespaces)
            line = line.trimmingCharacters(in: CharacterSet(charactersIn: "#*_ ")).trimmingCharacters(in: .whitespaces)
            guard !line.isEmpty else { continue }
            if let (heading, rest) = heading(of: line) {
                current = heading
                headings += 1
                let rest = clean(rest)
                if heading == .title { title = rest.isEmpty ? nil : rest }
                else if heading == .summary { if !rest.isEmpty { summary.append(rest) } }
                else if !rest.isEmpty, !isNone(rest) { lists[heading, default: []].append(rest) }
                continue
            }
            switch current {
            case .title?:
                if title == nil { title = clean(line) }
            case .summary?:
                summary.append(clean(line))
            case let heading?:
                let item = clean(bullet(line))
                if !item.isEmpty, !isNone(item) { lists[heading, default: []].append(item) }
            case nil:
                continue
            }
        }
        guard headings >= 2 else { return nil }
        let notes = MeetingNotes(
            summary: summary.joined(separator: " ").trimmingCharacters(in: .whitespaces),
            keyPoints: dedupe(lists[.keyPoints] ?? []),
            decisions: dedupe(lists[.decisions] ?? []),
            actions: dedupe(lists[.actions] ?? []),
            generator: generator
        )
        guard !notes.isEmpty else { return nil }
        return (title.map { String($0.prefix(80)) }, notes)
    }

    /// Whether notes from a model look real: something in them, and not just the transcript
    /// played back (more than 80% of its length).
    public static func isUsable(_ notes: MeetingNotes, transcriptWords: Int) -> Bool {
        guard !notes.isEmpty, !notes.summary.isEmpty || !notes.keyPoints.isEmpty else { return false }
        return notes.wordCount <= max(Int(Double(transcriptWords) * 0.8), 120)
    }

    // MARK: Without a model

    /// Notes from plain rules, for when no language model is available: sentences that sound
    /// like a task or a decision, and the longest other sentences as key points.
    public static func extractive(_ segments: [MeetingSegment]) -> MeetingNotes {
        let sentences = MeetingFormat.turns(segments)
            .flatMap { SentenceChunker.sentences($0.text) }
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { WordCount.count($0) >= 4 }
        let actions = dedupe(sentences.filter { matches($0, actionPattern) }).prefix(8)
        let decisions = dedupe(sentences.filter { matches($0, decisionPattern) && !actions.contains($0) }).prefix(5)
        let taken = Set(actions).union(decisions)
        let longest = sentences
            .filter { !taken.contains($0) && WordCount.count($0) >= 8 }
            .enumerated()
            .sorted { WordCount.count($0.element) > WordCount.count($1.element) }
            .prefix(5)
            .sorted { $0.offset < $1.offset }
            .map(\.element)
        let summary = sentences.filter { WordCount.count($0) >= 6 }.prefix(2).joined(separator: " ")
        return MeetingNotes(
            summary: summary, keyPoints: dedupe(longest), decisions: Array(decisions), actions: Array(actions),
            generator: .extractive
        )
    }

    static let actionPattern = #"\b(i'll|i will|we'll|we will|you'll|let's|let us|we need to|i need to|you need to|we should|can you|could you|please|action item|follow up|follow-up|next step|next steps|to-do|todo|by (monday|tuesday|wednesday|thursday|friday|saturday|sunday|tomorrow|tonight|next week|the end of))\b"#
    static let decisionPattern = #"\b(we decided|decided to|we agreed|agreed to|agreed that|let's go with|we're going with|we are going with|we'll go with|final decision|the decision is|settled on)\b"#

    static func matches(_ sentence: String, _ pattern: String) -> Bool {
        sentence.range(of: pattern, options: [.regularExpression, .caseInsensitive]) != nil
    }

    // MARK: Helpers

    enum Heading { case title, summary, keyPoints, decisions, actions }

    static func heading(of line: String) -> (Heading, String)? {
        let table: [(String, Heading)] = [
            ("title", .title), ("summary", .summary), ("key points", .keyPoints),
            ("decisions", .decisions), ("action items", .actions), ("actions", .actions),
        ]
        let lower = line.lowercased()
        for (name, heading) in table where lower.hasPrefix(name) {
            let rest = line.dropFirst(name.count)
            guard rest.isEmpty || rest.first == ":" else { continue }
            return (heading, String(rest.drop(while: { $0 == ":" || $0 == "*" || $0 == " " })))
        }
        return nil
    }

    static func bullet(_ line: String) -> String {
        var text = Substring(line)
        if let first = text.first, "-•*–".contains(first) {
            text = text.dropFirst()
        } else if let dot = text.firstIndex(where: { $0 == "." || $0 == ")" }), dot > text.startIndex,
                  text[text.startIndex..<dot].allSatisfy(\.isNumber) {
            text = text[text.index(after: dot)...]
        }
        if text.hasPrefix(" [ ]") || text.hasPrefix("[ ]") {
            text = text.drop(while: { $0 == " " }).dropFirst(3)
        }
        return String(text)
    }

    static func clean(_ text: String) -> String {
        text.trimmingCharacters(in: CharacterSet(charactersIn: "*_\"").union(.whitespaces))
    }

    static func isNone(_ text: String) -> Bool {
        let lower = text.lowercased().trimmingCharacters(in: CharacterSet(charactersIn: ". "))
        return ["none", "n/a", "no decisions", "no action items", "nothing", "none mentioned", "none were made"].contains(lower)
    }

    static func dedupe(_ items: [String]) -> [String] {
        var seen = Set<String>()
        return items.filter { seen.insert($0.lowercased()).inserted }
    }
}
