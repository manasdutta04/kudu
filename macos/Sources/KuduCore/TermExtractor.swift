import Foundation

/// Picks the names and code terms out of the text visible on screen, so dictation can spell
/// them the way they're written there: "Siddharth", "GitHub", "userService.ts", "@priya".
public enum TermExtractor {
    public static let maximumTerms = 40

    public static func terms(from texts: [String]) -> [String] {
        var counts: [String: Int] = [:]
        for text in texts {
            var previousEndsSentence = true
            for raw in text.split(whereSeparator: { $0.isWhitespace }) {
                let word = raw.trimmingCharacters(in: CharacterSet(charactersIn: ".,;:!?\"'()[]{}<>“”‘’"))
                defer { previousEndsSentence = raw.last.map { ".!?\n".contains($0) } ?? false }
                guard word.count >= 3, word.count <= 40, word.first?.isLetter == true || word.first == "@" else { continue }
                let bare = word.hasPrefix("@") ? String(word.dropFirst()) : word
                guard bare.count >= 3, !bare.contains("/"), !bare.contains("://"),
                      bare.allSatisfy({ $0.isLetter || $0.isNumber || "._-".contains($0) })
                else { continue }
                if isTerm(bare, handle: word.hasPrefix("@"), sentenceStart: previousEndsSentence) {
                    counts[bare, default: 0] += 1
                }
            }
        }
        return counts.sorted { $0.value != $1.value ? $0.value > $1.value : $0.key < $1.key }
            .prefix(maximumTerms).map(\.key)
    }

    static func isTerm(_ word: String, handle: Bool, sentenceStart: Bool) -> Bool {
        if handle { return true }
        let letters = word.filter(\.isLetter)
        guard !letters.isEmpty else { return false }
        // Code: userService.ts, session_token, fetchWithAuth.
        if word.contains("."), word.split(separator: ".").count >= 2, word.last?.isLetter == true { return true }
        if word.contains("_") { return true }
        if word.dropFirst().contains(where: \.isUppercase), word.contains(where: \.isLowercase) { return true }
        // Names: capitalized words not at a sentence start, and not common words.
        guard let first = word.first, first.isUppercase, !sentenceStart,
              word.dropFirst().allSatisfy({ $0.isLowercase || $0 == "-" || $0 == "'" })
        else { return false }
        return !commonCapitalized.contains(word.lowercased())
    }

    static let commonCapitalized: Set<String> = [
        "the", "this", "that", "these", "those", "and", "but", "for", "you", "your", "our", "his", "her", "its",
        "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday", "january", "february",
        "march", "april", "june", "july", "august", "september", "october", "november", "december", "today",
        "tomorrow", "yesterday", "new", "open", "save", "edit", "view", "file", "help", "window", "search",
        "settings", "home", "inbox", "draft", "drafts", "sent", "reply", "send", "cancel", "done", "ok",
    ]
}
