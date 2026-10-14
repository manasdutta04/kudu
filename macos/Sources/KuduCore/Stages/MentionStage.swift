import Foundation

/// "tag Priya" becomes "@Priya", the way people mention someone in Slack, Teams, Notion or a
/// doc's comments. Not in email, where there are no mentions, or in code.
public struct MentionStage: TextStage {
    private static let tag = try! NSRegularExpression(pattern: "\\b[Tt]ag @?([A-Za-z][\\w'’-]+)")

    private static let notNames: Set<String> = [
        "along", "team", "everyone", "everybody", "someone", "somebody", "people", "line", "name", "them", "him", "her", "me",
        "us", "all", "both", "each", "whoever", "anyone",
    ]

    public init() {}

    public func apply(_ text: String, context: TextContext) -> String {
        guard context.category != .email, context.category != .code else { return text }
        var result = text
        for match in Self.tag.matches(in: text, range: NSRange(text.startIndex..., in: text)).reversed() {
            guard let whole = Range(match.range, in: result), let name = Range(match.range(at: 1), in: result) else { continue }
            // "tag along", "tag me", "tag everyone in it": not a person's name.
            guard !Self.notNames.contains(result[name].lowercased()),
                  !CorrectionLearner.stopwords.contains(result[name].lowercased())
            else { continue }
            let person = result[name].prefix(1).uppercased() + result[name].dropFirst()
            result.replaceSubrange(whole, with: "@" + person)
        }
        return result
    }
}
