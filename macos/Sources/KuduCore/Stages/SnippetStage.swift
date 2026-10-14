import Foundation

/// A spoken shortcut that expands to saved text, e.g. "my email" → "me@example.com".
public struct Snippet: Codable, Sendable, Hashable, Identifiable {
    public var id: UUID
    public var trigger: String
    /// May contain `{date}`, `{time}` and `{clipboard}` placeholders.
    public var expansion: String

    public init(id: UUID = UUID(), trigger: String, expansion: String) {
        self.id = id
        self.trigger = trigger
        self.expansion = expansion
    }
}

/// Expands a snippet when the whole dictation is its trigger ("My address." → the address).
/// Matching the whole utterance keeps ordinary sentences that contain the words safe.
public struct SnippetStage: TextStage {
    private let snippets: [String: String]
    private let now: @Sendable () -> Date
    private let clipboard: @Sendable () -> String?

    public init(
        snippets: [Snippet],
        now: @escaping @Sendable () -> Date = Date.init,
        clipboard: @escaping @Sendable () -> String? = { nil }
    ) {
        var table: [String: String] = [:]
        for snippet in snippets where !snippet.trigger.isEmpty {
            table[Self.key(snippet.trigger)] = snippet.expansion
        }
        self.snippets = table
        self.now = now
        self.clipboard = clipboard
    }

    public func apply(_ text: String, context: TextContext) -> String {
        guard !snippets.isEmpty, let expansion = snippets[Self.key(text)] else { return text }
        return fill(expansion)
    }

    private func fill(_ template: String) -> String {
        guard template.contains("{") else { return template }
        let date = now()
        var result = template
        result = result.replacingOccurrences(
            of: "{date}", with: date.formatted(date: .long, time: .omitted))
        result = result.replacingOccurrences(
            of: "{time}", with: date.formatted(date: .omitted, time: .shortened))
        if result.contains("{clipboard}") {
            result = result.replacingOccurrences(of: "{clipboard}", with: clipboard() ?? "")
        }
        return result
    }

    /// "My Email." / "my email" / " my  email! " all map to "my email".
    static func key(_ text: String) -> String {
        text.lowercased()
            .components(separatedBy: CharacterSet.alphanumerics.union(.whitespaces).inverted).joined()
            .split(whereSeparator: \.isWhitespace).joined(separator: " ")
    }
}
