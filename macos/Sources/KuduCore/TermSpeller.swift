import Foundation

/// Fixes words the speech model misheard when they sound like a term the user cares about:
/// a dictionary word, or a name or term visible on screen.
///
/// - "hitmap" → "heatmap", "Sidarth" → "Siddharth": a word the spell checker doesn't know,
///   that sounds like a term.
/// - "super base" → "Supabase", "heat map" → "heatmap": two or three words that together
///   sound like a dictionary term written as one.
/// - "Pia" → "Priya": a capitalized name that sounds like a name in the dictionary.
/// - "github" → "GitHub": the same word with the term's inner capitals.
///
/// Real words are never turned into terms ("pride" stays "pride" next to "Priya"), except by
/// joining, which needs a close match to a dictionary term.
public struct TermSpeller {
    /// Terms the user added or Kudu learned: trusted, so matched more readily.
    public var dictionary: [String]
    /// Names and terms read from the screen.
    public var screen: [String]
    /// Whether a word is ordinary vocabulary (the system spell checker on macOS).
    public var isKnownWord: (String) -> Bool

    public init(dictionary: [String], screen: [String] = [], isKnownWord: @escaping (String) -> Bool) {
        self.dictionary = dictionary.filter { !$0.isEmpty }
        self.screen = screen.filter { !$0.isEmpty && !$0.contains(".") && !$0.contains("_") }
        self.isKnownWord = isKnownWord
    }

    private struct Word {
        var range: Range<String.Index>
        var text: String
    }

    private static let wordPattern = try! NSRegularExpression(pattern: "\\p{L}[\\p{L}'’]*")

    /// The corrected text and how many corrections were made.
    public func apply(_ text: String) -> (text: String, fixes: Int) {
        guard !dictionary.isEmpty || !screen.isEmpty else { return (text, 0) }
        let words: [Word] = Self.wordPattern.matches(in: text, range: NSRange(text.startIndex..., in: text)).compactMap {
            guard let range = Range($0.range, in: text) else { return nil }
            return Word(range: range, text: String(text[range]))
        }
        var replacements: [(Range<String.Index>, String)] = []
        var index = 0
        while index < words.count {
            if let (count, term) = joined(words, at: index, in: text) {
                replacements.append((words[index].range.lowerBound..<words[index + count - 1].range.upperBound, term))
                index += count
                continue
            }
            let word = words[index]
            let atSentenceStart = Self.isSentenceStart(word.range.lowerBound, in: text)
            if let term = single(word.text, atSentenceStart: atSentenceStart), term != word.text {
                replacements.append((word.range, term))
            }
            index += 1
        }
        guard !replacements.isEmpty else { return (text, 0) }
        var result = text
        for (range, term) in replacements.reversed() { result.replaceSubrange(range, with: term) }
        return (result, replacements.count)
    }

    // MARK: - Matching

    /// Two or three neighbouring words that together are a dictionary term ("heat map").
    private func joined(_ words: [Word], at index: Int, in text: String) -> (Int, String)? {
        for count in [3, 2] where index + count <= words.count {
            let run = words[index..<(index + count)]
            // Only words separated by spaces or hyphens, within one phrase.
            let gapsAreSpaces = zip(run, run.dropFirst()).allSatisfy { left, right in
                text[left.range.upperBound..<right.range.lowerBound].allSatisfy { $0 == " " || $0 == "-" }
            }
            guard gapsAreSpaces else { continue }
            // "the", "a", "to" are never part of a term that was split apart.
            guard !run.contains(where: { CorrectionLearner.stopwords.contains($0.text.lowercased()) && $0.text.count > 1 })
            else { continue }
            let letters = Self.letters(run.map(\.text).joined())
            // Spelled-out letters ("Q V") can only be matched by sound; words need a close spelling.
            let spelledOut = run.allSatisfy { $0.text.count == 1 }
            for term in dictionary where !term.contains(" ") && term.count >= 4 {
                let target = Self.letters(term)
                guard abs(target.count - letters.count) <= 3 else { continue }
                if letters == target || Self.similarity(letters, target) >= 0.75 || (spelledOut && Self.sameSound(letters, target)) {
                    return (count, term)
                }
            }
        }
        return nil
    }

    private func single(_ word: String, atSentenceStart: Bool) -> String? {
        guard word.count >= 2 else { return nil }
        let lower = word.lowercased()
        let all = dictionary + screen
        // Already a term, maybe in the wrong case.
        if let exact = all.first(where: { $0.lowercased() == lower }) {
            if exact == word { return nil }
            let innerCapitals = exact.dropFirst().contains(where: \.isUppercase)
            let trustedName = dictionary.contains(exact) && exact.first?.isUppercase == true && !atSentenceStart
            return innerCapitals || trustedName ? exact : nil
        }
        guard !CorrectionLearner.stopwords.contains(lower) else { return nil }

        let letters = Self.letters(word)
        func closest(in terms: [String], minimum: Double) -> String? {
            terms
                .filter { !$0.contains(" ") && abs($0.count - word.count) <= 3 }
                .map { ($0, Self.similarity(letters, Self.letters($0))) }
                .filter { $0.1 >= minimum || (Self.sameSound(letters, Self.letters($0.0)) && $0.1 >= 0.4) }
                .max { $0.1 < $1.1 }?.0
        }

        if !isKnownWord(word) {
            return closest(in: dictionary, minimum: 0.5) ?? closest(in: screen, minimum: 0.5)
        }
        // A capitalized word mid-sentence is a name: the speech model heard some name. If it
        // sounds like a name in the dictionary, it was most likely that one ("Pia" → "Priya").
        if word.first?.isUppercase == true, !atSentenceStart, word.count >= 3 {
            let names = dictionary.filter { $0.first?.isUppercase == true }
            return closest(in: names, minimum: 0.6)
        }
        return nil
    }

    // MARK: - Helpers

    static func letters(_ text: String) -> String {
        String(text.lowercased().filter(\.isLetter))
    }

    static func similarity(_ a: String, _ b: String) -> Double {
        guard !a.isEmpty, !b.isEmpty else { return 0 }
        let distance = FormattingGuard.editDistance(a.map(String.init), b.map(String.init))
        return 1 - Double(distance) / Double(max(a.count, b.count))
    }

    static func sameSound(_ a: String, _ b: String) -> Bool {
        let key = CorrectionLearner.phoneticKey(a)
        return key.count >= 2 && key == CorrectionLearner.phoneticKey(b)
    }

    private static func isSentenceStart(_ position: String.Index, in text: String) -> Bool {
        let before = text[..<position].trimmingCharacters(in: .whitespaces)
        guard let last = before.last else { return true }
        return ".?!:\n•-".contains(last)
    }
}
