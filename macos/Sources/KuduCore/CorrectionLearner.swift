import Foundation

/// A spelling Kudu should remember: it typed `heard`, the user corrected it to `written`.
public struct LearnedCorrection: Equatable, Sendable {
    public var heard: String
    public var written: String
}

/// Learns from the corrections people make right after a dictation. It word-aligns what Kudu
/// typed with what the field says now and keeps only edits that look like a fixed spelling of
/// a name or term, never rewording.
///
/// - "I pushed it to git hub" → "I pushed it to GitHub": learns git hub → GitHub.
/// - "Thanks kudoo team" → "Thanks Kudu team": learns kudoo → Kudu.
/// - "the" → "a", "send it" → "email it", or a rewritten sentence: learns nothing.
public enum CorrectionLearner {
    /// More than this share of words changed means the user rewrote the text, not fixed it.
    static let maximumChangedShare = 0.5
    static let maximumCandidates = 3
    static let maximumPhraseWords = 3

    /// - Parameter isKnownWord: whether a word is ordinary vocabulary (the system spell checker).
    ///   Swapping one ordinary word for another ("hit" → "heat") is only safe with its
    ///   neighbour, so it's learned as a phrase ("hit map" → "heat map"); learning the bare
    ///   word would change every "hit" from then on.
    public static func learn(
        inserted: String, edited: String, known: Set<String> = [], isKnownWord: (String) -> Bool = { _ in false }
    ) -> [LearnedCorrection] {
        let before = tokens(inserted)
        let after = tokens(edited)
        guard !before.isEmpty, !after.isEmpty, before != after else { return [] }

        let operations = align(before, after)
        let changed = operations.filter { $0.kind != .match }.count
        guard Double(changed) / Double(before.count) <= maximumChangedShare else { return [] }

        let knownLowercased = Set(known.map { $0.lowercased() })
        var results: [LearnedCorrection] = []
        var index = 0
        while index < operations.count {
            guard operations[index].kind != .match else {
                index += 1
                continue
            }
            // A run of edits between two matching words (or the text's edges).
            var end = index
            while end < operations.count, operations[end].kind != .match { end += 1 }
            let run = operations[index..<end]
            let from = run.compactMap(\.before)
            let to = run.compactMap(\.after)
            let atSentenceStart = index == 0
                || operations[index - 1].before.map { ".?!:".contains($0.last ?? " ") } ?? false
            if var correction = candidate(from: from, to: to, atSentenceStart: atSentenceStart) {
                let ordinarySwap = from.count == 1 && to.count == 1
                    && correction.heard != correction.written.lowercased()
                    && isKnownWord(from[0]) && isKnownWord(to[0])
                if ordinarySwap {
                    // Anchor it to the unchanged word after it (or before it, at the end).
                    if end < operations.count, let next = operations[end].after {
                        correction = LearnedCorrection(heard: correction.heard + " " + next.lowercased(), written: correction.written + " " + next)
                    } else if index > 0, let previous = operations[index - 1].after {
                        correction = LearnedCorrection(heard: previous.lowercased() + " " + correction.heard, written: previous + " " + correction.written)
                    } else {
                        index = end
                        continue
                    }
                }
                if !knownLowercased.contains(correction.written.lowercased()),
                   !results.contains(where: { $0.written == correction.written }) {
                    results.append(correction)
                }
            }
            index = end
        }
        return Array(results.prefix(maximumCandidates))
    }

    // MARK: - Deciding

    private static func candidate(from: [String], to: [String], atSentenceStart: Bool) -> LearnedCorrection? {
        guard (1...maximumPhraseWords).contains(from.count), (1...maximumPhraseWords).contains(to.count) else { return nil }
        let heard = from.map(bare).joined(separator: " ")
        let written = to.map(bare).joined(separator: " ")
        guard !heard.isEmpty, !written.isEmpty, heard != written else { return nil }

        // Numbers, contractions and everyday words aren't vocabulary.
        let words = (heard + " " + written).split(separator: " ").map { $0.lowercased() }
        guard !words.contains(where: { $0.contains(where: \.isNumber) }) else { return nil }
        guard !written.contains("'"), !written.contains("’") else { return nil }
        if to.count == 1, stopwords.contains(written.lowercased()) { return nil }
        if from.count == 1, to.count == 1, stopwords.contains(heard.lowercased()) { return nil }

        if heard.lowercased() == written.lowercased() {
            // A capitalization fix. At a sentence start it says nothing about the word itself,
            // unless the new form has a capital inside ("GitHub", "iPhone").
            let innerCapital = written.dropFirst().contains(where: \.isUppercase)
            guard written.contains(where: \.isUppercase), innerCapital || !atSentenceStart else { return nil }
            return LearnedCorrection(heard: heard.lowercased(), written: written)
        }
        guard soundsAlike(heard, written) else { return nil }
        return LearnedCorrection(heard: heard.lowercased(), written: written)
    }

    /// Whether `heard` is plausibly a mis-hearing or misspelling of `written`: close in spelling,
    /// or the same consonant sounds ("coo doo" / "Kudu", "priyah" / "Priya").
    public static func soundsAlike(_ heard: String, _ written: String) -> Bool {
        let a = letters(heard)
        let b = letters(written)
        guard !a.isEmpty, !b.isEmpty else { return false }
        let distance = FormattingGuard.editDistance(a.map(String.init), b.map(String.init))
        let similarity = 1 - Double(distance) / Double(max(a.count, b.count))
        if similarity >= 0.45 { return true }
        let keyA = phoneticKey(a)
        return keyA.count >= 2 && keyA == phoneticKey(b)
    }

    /// Soundex-style consonant skeleton, including the first letter: "coodoo" → "23", "kudu" → "23".
    static func phoneticKey(_ letters: String) -> String {
        let groups: [Character: Character] = [
            "b": "1", "f": "1", "p": "1", "v": "1",
            "c": "2", "g": "2", "j": "2", "k": "2", "q": "2", "s": "2", "x": "2", "z": "2",
            "d": "3", "t": "3", "l": "4", "m": "5", "n": "5", "r": "6",
        ]
        var key = ""
        var last: Character?
        for character in letters {
            let code = groups[character]
            if let code, code != last { key.append(code) }
            last = code
        }
        return key
    }

    private static func letters(_ text: String) -> String {
        String(text.lowercased().filter(\.isLetter))
    }

    // MARK: - Alignment

    private enum Kind { case match, substitute, insert, delete }

    private struct Operation {
        var kind: Kind
        var before: String?
        var after: String?
    }

    /// Words, keeping their case and inner punctuation ("GitHub", "don't"), without the
    /// punctuation around them.
    static func tokens(_ text: String) -> [String] {
        text.split(whereSeparator: \.isWhitespace).map(String.init).map(bare).filter { !$0.isEmpty }
    }

    private static func bare(_ word: String) -> String {
        word.trimmingCharacters(in: .punctuationCharacters.union(.symbols))
    }

    /// Word-level edit alignment with backtrace. Words compare exactly (case-sensitive), so a
    /// capitalization fix shows up as a substitution.
    private static func align(_ a: [String], _ b: [String]) -> [Operation] {
        var cost = Array(repeating: Array(repeating: 0, count: b.count + 1), count: a.count + 1)
        for i in 0...a.count { cost[i][0] = i }
        for j in 0...b.count { cost[0][j] = j }
        for i in stride(from: 1, through: a.count, by: 1) {
            for j in stride(from: 1, through: b.count, by: 1) {
                let same = a[i - 1] == b[j - 1]
                cost[i][j] = min(cost[i - 1][j] + 1, cost[i][j - 1] + 1, cost[i - 1][j - 1] + (same ? 0 : 1))
            }
        }
        var operations: [Operation] = []
        var i = a.count
        var j = b.count
        while i > 0 || j > 0 {
            if i > 0, j > 0, cost[i][j] == cost[i - 1][j - 1] + (a[i - 1] == b[j - 1] ? 0 : 1) {
                operations.append(Operation(kind: a[i - 1] == b[j - 1] ? .match : .substitute, before: a[i - 1], after: b[j - 1]))
                i -= 1
                j -= 1
            } else if i > 0, cost[i][j] == cost[i - 1][j] + 1 {
                operations.append(Operation(kind: .delete, before: a[i - 1], after: nil))
                i -= 1
            } else {
                operations.append(Operation(kind: .insert, before: nil, after: b[j - 1]))
                j -= 1
            }
        }
        return operations.reversed()
    }

    static let stopwords: Set<String> = [
        "a", "an", "the", "and", "or", "but", "if", "of", "to", "in", "on", "at", "by", "for", "with", "from",
        "is", "are", "was", "were", "be", "been", "am", "do", "does", "did", "have", "has", "had", "it", "its",
        "this", "that", "these", "those", "i", "you", "he", "she", "we", "they", "me", "him", "her", "us", "them",
        "my", "your", "his", "our", "their", "so", "not", "no", "yes", "as", "up", "out", "can", "will", "just",
        "then", "than", "there", "here", "what", "when", "where", "who", "how", "why", "all", "any", "some",
        "about", "into", "over", "also", "very", "too", "ok", "okay",
    ]
}
