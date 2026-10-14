import Foundation

/// Decides whether text returned by the language model is a faithful formatting of what was
/// dictated, or whether the model rewrote it, answered it, or wrapped it in commentary.
/// Formatting changes punctuation, casing, line breaks and number style — not the words.
public enum FormattingGuard {
    /// Largest share of words that may differ (edits, insertions, deletions).
    public static let maximumWordChange = 0.35
    /// Largest change in word count, as a share of the original.
    public static let maximumLengthChange = 0.20

    /// Returns the cleaned-up model output if it is acceptable, otherwise nil.
    public static func accept(original: String, formatted: String) -> String? {
        var output = formatted.trimmingCharacters(in: .whitespacesAndNewlines)
        // Models sometimes wrap the answer in quotes or a code fence.
        for (open, close) in [("\"", "\""), ("“", "”"), ("```", "```")] where output.hasPrefix(open) && output.hasSuffix(close) && output.count > open.count + close.count {
            output = String(output.dropFirst(open.count).dropLast(close.count)).trimmingCharacters(in: .whitespacesAndNewlines)
        }
        guard !output.isEmpty else { return nil }

        let before = words(original)
        let after = words(output)
        guard !before.isEmpty else { return nil }

        let lengthChange = abs(Double(after.count - before.count)) / Double(before.count)
        guard lengthChange <= maximumLengthChange else { return nil }

        let change = Double(editDistance(before, after)) / Double(before.count)
        guard change <= maximumWordChange else { return nil }
        return output
    }

    /// Lowercased word tokens without punctuation, so formatting-only changes compare equal.
    static func words(_ text: String) -> [String] {
        text.lowercased()
            .split(whereSeparator: { !($0.isLetter || $0.isNumber || $0 == "'" || $0 == "’") })
            .map { $0.replacingOccurrences(of: "’", with: "'") }
    }

    /// Word-level Levenshtein distance.
    static func editDistance(_ a: [String], _ b: [String]) -> Int {
        if a.isEmpty { return b.count }
        if b.isEmpty { return a.count }
        var previous = Array(0...b.count)
        var current = [Int](repeating: 0, count: b.count + 1)
        for i in 1...a.count {
            current[0] = i
            for j in 1...b.count {
                let cost = a[i - 1] == b[j - 1] ? 0 : 1
                current[j] = min(previous[j] + 1, current[j - 1] + 1, previous[j - 1] + cost)
            }
            swap(&previous, &current)
        }
        return previous[b.count]
    }
}
