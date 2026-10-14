import Foundation

/// A word or phrase the user wants spelled their way, e.g. "kudoo" / "coo doo" → "Kudu".
public struct DictionaryEntry: Codable, Sendable, Hashable, Identifiable {
    public var id: UUID
    /// How the speech model tends to hear it. Matched case-insensitively as whole words.
    public var heard: [String]
    /// How it should be written.
    public var written: String
    /// Learned automatically from a correction the user made. Nil in older files.
    public var learned: Bool?

    public init(id: UUID = UUID(), heard: [String], written: String, learned: Bool = false) {
        self.id = id
        self.heard = heard
        self.written = written
        self.learned = learned ? true : nil
    }

    public var isLearned: Bool { learned == true }
}

/// Replaces misheard words with the user's spelling. Also fixes the capitalization of the
/// written form itself ("kudu" → "Kudu", "github" → "GitHub").
public struct DictionaryStage: TextStage {
    private let rules: [(NSRegularExpression, String)]

    public init(entries: [DictionaryEntry]) {
        rules = entries.compactMap { entry in
            let variants = (entry.heard + [entry.written])
                .map { $0.trimmingCharacters(in: .whitespaces) }
                .filter { !$0.isEmpty }
            guard !variants.isEmpty, !entry.written.isEmpty else { return nil }
            // Longest first so "coo doo app" wins over "coo doo".
            let alternatives = variants
                .sorted { $0.count > $1.count }
                .map { NSRegularExpression.escapedPattern(for: $0).replacingOccurrences(of: " ", with: "[\\s-]+") }
                .joined(separator: "|")
            guard let regex = try? NSRegularExpression(
                pattern: "(?<![\\w'])(?:\(alternatives))(?![\\w'])", options: [.caseInsensitive]
            ) else { return nil }
            return (regex, NSRegularExpression.escapedTemplate(for: entry.written))
        }
    }

    /// How many words in `text` this dictionary would actually correct (spelling that differs
    /// from the user's, not words that are already right).
    public func corrections(in text: String) -> Int {
        rules.reduce(0) { count, rule in
            let range = NSRange(text.startIndex..., in: text)
            return count + rule.0.matches(in: text, range: range).filter { match in
                Range(match.range, in: text).map { text[$0] != rule.1 } ?? false
            }.count
        }
    }

    public func apply(_ text: String, context: TextContext) -> String {
        rules.reduce(text) { partial, rule in
            let range = NSRange(partial.startIndex..., in: partial)
            return rule.0.stringByReplacingMatches(in: partial, range: range, withTemplate: rule.1)
        }
    }
}
