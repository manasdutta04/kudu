import Foundation

/// Decides whether text returned by the language model is a faithful formatting of what was
/// dictated, or whether the model rewrote it, answered it, or wrapped it in commentary.
/// Formatting changes punctuation, casing, line breaks and number style — not the words.
public enum FormattingGuard {
    /// Returns the cleaned-up model output if it is acceptable, otherwise nil.
    public static func accept(original: String, formatted: String, strength: EditingStrength = .light) -> String? {
        var output = formatted.trimmingCharacters(in: .whitespacesAndNewlines)
        // Models sometimes wrap the answer in quotes or a code fence.
        for (open, close) in [("\"", "\""), ("“", "”"), ("```", "```")] where output.hasPrefix(open) && output.hasSuffix(close) && output.count > open.count + close.count {
            output = String(output.dropFirst(open.count).dropLast(close.count)).trimmingCharacters(in: .whitespacesAndNewlines)
        }
        guard !output.isEmpty else { return nil }

        // Spoken math becomes notation ("x squared" → "x²"): judged by its own rule.
        if FormattingNeed.hasMathCues(original) { return acceptMath(original: original, formatted: output) }

        var before = words(original)
        // List markers ("1.", "•") are layout, not words.
        var after = words(output.replacingOccurrences(of: "(^|\\n)\\s*(\\d+[.)]|•|-)\\s+", with: "$1", options: .regularExpression))
        // A list drops its spoken enumerators ("three things first… second…", "add …") for
        // bullets or one item per line.
        let lines = output.split(separator: "\n").filter { !$0.trimmingCharacters(in: .whitespaces).isEmpty }
        if output.contains("•") || lines.count >= 3
            || output.range(of: "(^|\\n)\\s*\\d+[.)]", options: .regularExpression) != nil {
            before.removeAll { enumerators.contains($0) }
            after.removeAll { enumerators.contains($0) }
        }
        guard !before.isEmpty else { return nil }

        let lengthChange = abs(Double(after.count - before.count)) / Double(before.count)
        guard lengthChange <= strength.maximumLengthChange else { return nil }

        let change = Double(editDistance(before, after)) / Double(before.count)
        guard change <= strength.maximumWordChange else { return nil }
        return output
    }

    static let enumerators: Set<String> = [
        "first", "firstly", "second", "secondly", "third", "thirdly", "fourth", "fifth", "finally", "next",
        "number", "one", "two", "three", "four", "five", "things", "items", "points", "and", "also", "then", "add",
    ]

    /// Math notation replaces spoken words with symbols, so it can't be compared word by word.
    /// It is accepted when every word left in it was spoken (or is a math name like sin) and it
    /// isn't much longer than what was said: an answer or an explanation would fail both.
    static func acceptMath(original: String, formatted: String) -> String? {
        // Letters only: "x²" and "2x" are notation around a spoken "x".
        func letterWords(_ text: String) -> [String] {
            text.lowercased().split(whereSeparator: { !$0.isLetter }).map(String.init)
        }
        let spoken = Set(letterWords(original))
        let mathNames: Set<String> = ["sin", "cos", "tan", "log", "ln", "lim", "d", "dx", "dy", "dt", "e", "pi"]
        let unexpected = letterWords(formatted).filter { word in
            !spoken.contains(word) && !mathNames.contains(word) && !(word.count == 1)
                && !word.allSatisfy(\.isNumber) && !spoken.contains(where: { $0.hasPrefix(word) || word.hasPrefix($0) })
        }
        guard unexpected.isEmpty, formatted.count <= original.count * 3 / 2 + 10 else { return nil }
        return formatted
    }

    /// Code formatting legitimately joins and symbolizes words ("user service dot ts" →
    /// "userService.ts"), so code output is compared by its letters instead: once spoken
    /// symbol words are dropped, the letters must still be essentially the same.
    public static func acceptCode(original: String, formatted: String) -> String? {
        var output = formatted.trimmingCharacters(in: .whitespacesAndNewlines)
        if output.hasPrefix("```"), output.hasSuffix("```"), output.count > 6 {
            output = String(output.dropFirst(3).dropLast(3))
            if let newline = output.firstIndex(of: "\n"), !output[..<newline].contains(" ") {
                output = String(output[output.index(after: newline)...])
            }
            output = output.trimmingCharacters(in: .whitespacesAndNewlines)
        }
        if output.hasPrefix("$ ") { output.removeFirst(2) }
        guard !output.isEmpty else { return nil }
        let spokenSymbols: Set<String> = [
            "dot", "dash", "hyphen", "underscore", "slash", "backslash", "equals", "quote", "quotes", "colon",
            "semicolon", "comma", "period", "space", "camel", "case", "snake", "open", "close", "paren", "bracket",
        ]
        func letters(_ text: String, dropping symbols: Bool) -> [String] {
            words(text).filter { !symbols || !spokenSymbols.contains($0) }.joined().map(String.init)
        }
        let before = letters(original, dropping: true)
        let after = letters(output, dropping: false)
        guard !before.isEmpty else { return nil }
        let change = Double(editDistance(before, after)) / Double(before.count)
        return change <= 0.15 ? output : nil
    }

    /// Lowercased word tokens without punctuation, so formatting-only changes compare equal.
    /// Written and spoken forms of numbers and units count as the same word ("two" = "2",
    /// "kilo" = "kg"), and "8pm" counts as "8 pm".
    static func words(_ text: String) -> [String] {
        text.lowercased()
            .split(whereSeparator: { !($0.isLetter || $0.isNumber || $0 == "'" || $0 == "’") })
            .flatMap { splitDigitsAndLetters(String($0)) }
            .map { $0.replacingOccurrences(of: "’", with: "'") }
            .compactMap(canonical)
    }

    private static func splitDigitsAndLetters(_ token: String) -> [String] {
        var parts: [String] = []
        var current = ""
        for character in token {
            if let last = current.last, last.isNumber != character.isNumber {
                parts.append(current)
                current = ""
            }
            current.append(character)
        }
        if !current.isEmpty { parts.append(current) }
        return parts
    }

    private static let numberWords: [String: String] = [
        "zero": "0", "one": "1", "two": "2", "three": "3", "four": "4", "five": "5", "six": "6", "seven": "7",
        "eight": "8", "nine": "9", "ten": "10", "eleven": "11", "twelve": "12", "fifteen": "15", "twenty": "20",
        "thirty": "30", "forty": "40", "fifty": "50", "hundred": "100", "thousand": "1000",
    ]
    private static let unitWords: [String: String] = [
        "kilo": "kg", "kilos": "kg", "kilogram": "kg", "kilograms": "kg", "gram": "g", "grams": "g",
        "litre": "l", "liter": "l", "litres": "l", "liters": "l", "kilometer": "km", "kilometre": "km",
        "kilometers": "km", "kilometres": "km", "minutes": "min",
    ]
    /// Words that become symbols ($, %, :) when written, so they disappear from the text.
    private static let symbolWords: Set<String> = ["percent", "dollars", "dollar", "rupees", "euros", "pounds", "o'clock"]

    private static func canonical(_ word: String) -> String? {
        if symbolWords.contains(word) { return nil }
        if let digits = numberWords[word] { return digits }
        return unitWords[word] ?? word
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
