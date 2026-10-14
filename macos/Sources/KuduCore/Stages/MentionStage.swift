import Foundation

/// In chat apps, "tag Priya" becomes "@Priya", the way people mention someone in Slack or
/// Teams. Only in messaging apps, where a mention is what "tag" means.
public struct MentionStage: TextStage {
    private static let tag = try! NSRegularExpression(pattern: "\\b[Tt]ag @?([A-Za-z][\\w'’-]+)")

    public init() {}

    public func apply(_ text: String, context: TextContext) -> String {
        guard context.category.isMessage else { return text }
        var result = text
        for match in Self.tag.matches(in: text, range: NSRange(text.startIndex..., in: text)).reversed() {
            guard let whole = Range(match.range, in: result), let name = Range(match.range(at: 1), in: result) else { continue }
            let person = result[name].prefix(1).uppercased() + result[name].dropFirst()
            result.replaceSubrange(whole, with: "@" + person)
        }
        return result
    }
}
