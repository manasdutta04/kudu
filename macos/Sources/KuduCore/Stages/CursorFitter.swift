import Foundation

/// Makes the inserted text read naturally where the cursor is:
/// - adds a leading space when continuing after a word,
/// - lowercases the first letter when continuing mid-sentence,
/// - in code contexts, drops the trailing period ASR models add to every utterance.
public struct CursorFitter: TextStage {
    public init() {}

    public func apply(_ text: String, context: TextContext) -> String {
        guard !text.isEmpty else { return text }
        var result = text

        if context.category == .code, result.hasSuffix("."), !result.hasSuffix("..") {
            result.removeLast()
        }

        guard let previous = context.precedingCharacter else { return result }

        let continuesSentence = previous.isLetter || previous.isNumber || previous == ","
        if continuesSentence, let first = result.first, first.isUppercase, !Self.keepsCapital(result) {
            result = first.lowercased() + result.dropFirst()
        }
        if !previous.isWhitespace, previous != "(", previous != "[", previous != "\"",
           let first = result.first, !first.isPunctuation {
            result = " " + result
        }
        return result
    }

    /// "I", "I'm" and acronyms keep their capitals mid-sentence.
    private static func keepsCapital(_ text: String) -> Bool {
        let firstWord = text.prefix { $0.isLetter || $0 == "'" }
        if firstWord == "I" || firstWord.hasPrefix("I'") { return true }
        return firstWord.count > 1 && firstWord.allSatisfy(\.isUppercase)
    }
}
