import Foundation

/// Collapses runs of spaces, removes spaces before punctuation, and trims the ends.
public struct WhitespaceNormalizer: TextStage {
    public init() {}

    public func apply(_ text: String, context: TextContext) -> String {
        var result = text.replacingOccurrences(of: "[ \\t]{2,}", with: " ", options: .regularExpression)
        result = result.replacingOccurrences(of: " +([,.;:!?])", with: "$1", options: .regularExpression)
        return result.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
