import Foundation

/// Turns spoken layout commands into formatting: "new line" → line break,
/// "new paragraph" → blank line. The speech model punctuates these like words
/// ("Thanks. New paragraph. Best,"), so the surrounding punctuation is absorbed too.
public struct SpokenCommands: TextStage {
    private static let commands: [(pattern: String, replacement: String)] = [
        ("new paragraph", "\n\n"),
        ("new line", "\n"),
        ("next line", "\n"),
    ]

    private let regexes: [(NSRegularExpression, String)]

    public init() {
        regexes = Self.commands.map { command in
            // Optional trailing comma before the command (the preceding sentence keeps its own
            // period), the command itself, then any punctuation and spaces after it.
            let regex = try! NSRegularExpression(
                pattern: ",?\\s*\\b\(command.pattern)\\b[,.!?]*\\s*", options: [.caseInsensitive]
            )
            return (regex, command.replacement)
        }
    }

    public func apply(_ text: String, context: TextContext) -> String {
        var result = text
        var changed = false
        for (regex, replacement) in regexes {
            let range = NSRange(result.startIndex..., in: result)
            guard regex.firstMatch(in: result, range: range) != nil else { continue }
            result = regex.stringByReplacingMatches(
                in: result, range: range, withTemplate: NSRegularExpression.escapedTemplate(for: replacement)
            )
            changed = true
        }
        guard changed else { return text }
        // Text after a break starts a new sentence.
        return result.capitalizingSentenceStarts()
    }
}
