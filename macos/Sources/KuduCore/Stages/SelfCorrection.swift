import Foundation

/// Applies the corrections people make out loud while dictating:
/// - "Let's meet Tuesday, no, Wednesday" → "Let's meet Wednesday"
/// - "at 3, sorry, 4 pm" → "at 4 pm"
/// - "Send it to Alex. Scratch that. Send it to Sam." → "Send it to Sam."
///
/// Deliberately conservative: a correction cue only counts when the speech model has set it
/// off with commas (or as its own sentence for "scratch that"), and only the single word
/// before the cue is replaced. Ordinary uses of "no" or "sorry" are left alone.
public struct SelfCorrection: TextStage {
    private let inlineCue: NSRegularExpression
    private let scratchThat: NSRegularExpression

    public init() {
        // <word>, no, | <word>, sorry, | <word>, no wait, | <word>, wait, no,
        inlineCue = try! NSRegularExpression(
            pattern: "\\b[\\w'’]+,\\s+(?:no|sorry|no wait|wait,? no|or rather),\\s+", options: [.caseInsensitive]
        )
        // A sentence (or the start of the text) followed by "scratch that" / "delete that".
        scratchThat = try! NSRegularExpression(
            pattern: "(?:^|(?<=[.?!]\\s))[^.?!]*[.?!,]?\\s*\\b(?:scratch that|delete that)\\b[.,!]?\\s*",
            options: [.caseInsensitive]
        )
    }

    public func apply(_ text: String, context: TextContext) -> String {
        var result = replaceAll(scratchThat, in: text)
        result = replaceAll(inlineCue, in: result)
        guard result != text else { return text }
        return result.trimmingCharacters(in: .whitespaces).capitalizingSentenceStarts()
    }

    private func replaceAll(_ regex: NSRegularExpression, in text: String) -> String {
        let range = NSRange(text.startIndex..., in: text)
        return regex.stringByReplacingMatches(in: text, range: range, withTemplate: "")
    }
}
