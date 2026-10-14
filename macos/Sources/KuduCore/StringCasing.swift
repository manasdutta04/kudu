import Foundation

extension String {
    /// Uppercases the first letter of the string and of every sentence that follows ". ", "? ",
    /// "! " or a line break. Leaves everything else untouched.
    func capitalizingSentenceStarts() -> String {
        var result = ""
        result.reserveCapacity(count)
        var atSentenceStart = true
        for character in self {
            if atSentenceStart, character.isLetter {
                result.append(contentsOf: character.uppercased())
                atSentenceStart = false
                continue
            }
            result.append(character)
            if ".?!\n".contains(character) {
                atSentenceStart = true
            } else if !character.isWhitespace {
                atSentenceStart = false
            }
        }
        return result
    }
}
