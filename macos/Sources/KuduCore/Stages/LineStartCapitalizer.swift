import Foundation

/// Capitalizes the first letter of every line after the first ("Dear Priya,\nthanks…" →
/// "Thanks…"). The first line is left to `CursorFitter`, which knows whether the text
/// continues a sentence already in the field.
public struct LineStartCapitalizer: TextStage {
    public init() {}

    public func apply(_ text: String, context: TextContext) -> String {
        guard text.contains("\n") else { return text }
        var result = ""
        result.reserveCapacity(text.count)
        var atLineStart = false
        for character in text {
            if atLineStart, character.isLetter {
                result.append(contentsOf: character.uppercased())
                atLineStart = false
                continue
            }
            result.append(character)
            if character == "\n" {
                atLineStart = true
            } else if !character.isWhitespace && !(atLineStart && (character.isNumber || ".-•)".contains(character))) {
                atLineStart = false
            }
        }
        return result
    }
}
