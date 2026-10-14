import Foundation

/// Removes hesitation sounds ("um", "uh", "erm", "hmm") along with the comma or period the
/// speech model attaches to them, and re-capitalizes a sentence that started with one.
public struct FillerRemover: TextStage {
    public static let defaultFillers = ["um", "umm", "uh", "uhh", "uhm", "erm", "er", "hmm", "mm"]

    private let regex: NSRegularExpression

    public init(fillers: [String] = FillerRemover.defaultFillers) {
        let alternatives = fillers.map(NSRegularExpression.escapedPattern(for:)).joined(separator: "|")
        // A filler is a whole word, optionally followed by "," "." or "…", plus the space after it.
        regex = try! NSRegularExpression(
            pattern: "\\b(?:\(alternatives))\\b[,.…]*\\s*", options: [.caseInsensitive]
        )
    }

    public func apply(_ text: String, context: TextContext) -> String {
        let range = NSRange(text.startIndex..., in: text)
        guard regex.firstMatch(in: text, range: range) != nil else { return text }
        var result = regex.stringByReplacingMatches(in: text, range: range, withTemplate: "")
        // "So, , we" → "So, we" when a filler sat between two commas.
        result = result.replacingOccurrences(of: ",\\s*,", with: ",", options: .regularExpression)
        result = result.trimmingCharacters(in: .whitespaces)
        if result.hasSuffix(",") { result.removeLast() }
        // "Ship it, uh." loses its final period along with the filler; put it back.
        if let terminal = text.last, ".?!".contains(terminal),
           let last = result.last, !last.isPunctuation {
            result.append(terminal)
        }
        return result.capitalizingSentenceStarts()
    }
}
