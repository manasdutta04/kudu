import Foundation

/// A word or phrase the user wants spelled their way, e.g. "kudoo" / "coo doo" → "Kudu".
public struct DictionaryEntry: Codable, Sendable, Hashable, Identifiable {
    public var id: UUID
    /// How the speech model tends to hear it. Matched case-insensitively as whole words.
    public var heard: [String]
    /// How it should be written.
    public var written: String

    public init(id: UUID = UUID(), heard: [String], written: String) {
        self.id = id
        self.heard = heard
        self.written = written
    }
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

    public func apply(_ text: String, context: TextContext) -> String {
        rules.reduce(text) { partial, rule in
            let range = NSRange(partial.startIndex..., in: partial)
            return rule.0.stringByReplacingMatches(in: partial, range: range, withTemplate: rule.1)
        }
    }
}
