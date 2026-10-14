import Foundation

/// Splits a transcript into sentence-aligned chunks for formatting while the user speaks.
/// Chunks are built greedily from the start, so the same prefix always gives the same chunks
/// (a chunk formatted earlier can be reused if the transcript up to it hasn't changed).
public enum SentenceChunker {
    /// Sentences, each keeping its punctuation: "Hi. How are you?" → ["Hi.", "How are you?"].
    public static func sentences(_ text: String) -> [String] {
        var sentences: [String] = []
        var current = ""
        let characters = Array(text)
        for (index, character) in characters.enumerated() {
            current.append(character)
            let ends = ".?!".contains(character)
            let nextIsBoundary = index + 1 == characters.count || characters[index + 1].isWhitespace
            // "3.30" or "e.g." in the middle of a word is not a sentence end.
            if ends, nextIsBoundary {
                let trimmed = current.trimmingCharacters(in: .whitespaces)
                if !trimmed.isEmpty { sentences.append(trimmed) }
                current = ""
            }
        }
        let rest = current.trimmingCharacters(in: .whitespaces)
        if !rest.isEmpty { sentences.append(rest) }
        return sentences
    }

    /// Groups sentences into chunks of at least `minimumWords` words. The final sentence is
    /// never part of a stable chunk while the user may still be speaking it; it is returned
    /// in `rest` together with any sentences that don't fill a chunk yet.
    public static func chunks(_ text: String, minimumWords: Int = 12) -> (stable: [String], rest: String) {
        var sentences = sentences(text)
        guard !sentences.isEmpty else { return ([], "") }
        let last = sentences.removeLast()
        var stable: [String] = []
        var current: [String] = []
        var words = 0
        for sentence in sentences {
            current.append(sentence)
            words += WordCount.count(sentence)
            if words >= minimumWords {
                stable.append(current.joined(separator: " "))
                current = []
                words = 0
            }
        }
        let rest = (current + [last]).joined(separator: " ")
        return (stable, rest)
    }
}
