import Foundation

/// Cleans what the language model returns in Command mode so only the text to insert is left:
/// no "Sure! Here's the revised text:" preamble, no wrapping quotes or code fences, no
/// "Let me know if…" afterword.
public enum CommandOutput {
    private static let preamble = try! NSRegularExpression(
        pattern: "^(sure|certainly|of course|absolutely|okay|ok|here(’|')?s|here is|here are|below is)\\b[^\\n]*?:\\s*\\n?",
        options: [.caseInsensitive])
    /// "Dear, may I…": a salutation with no name, which the model sometimes adds when it
    /// formalizes "hey".
    private static let namelessSalutation = try! NSRegularExpression(pattern: "^(?i:dear),\\s+(?=\\p{Ll})")
    private static let afterword = try! NSRegularExpression(
        pattern: "\\n+\\s*(let me know|i hope this|feel free to|is there anything)[^\\n]*$", options: [.caseInsensitive])

    public static func clean(_ output: String) -> String {
        var text = output.trimmingCharacters(in: .whitespacesAndNewlines)
        var range = NSRange(text.startIndex..., in: text)
        text = preamble.stringByReplacingMatches(in: text, range: range, withTemplate: "")
        range = NSRange(text.startIndex..., in: text)
        text = afterword.stringByReplacingMatches(in: text, range: range, withTemplate: "")
        range = NSRange(text.startIndex..., in: text)
        if namelessSalutation.firstMatch(in: text, range: range) != nil {
            text = namelessSalutation.stringByReplacingMatches(in: text, range: range, withTemplate: "")
            text = text.prefix(1).uppercased() + text.dropFirst()
        }
        text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        for (open, close) in [("```", "```"), ("\"", "\""), ("“", "”")]
        where text.hasPrefix(open) && text.hasSuffix(close) && text.count > open.count + close.count {
            text = String(text.dropFirst(open.count).dropLast(close.count))
            if open == "```", let newline = text.firstIndex(of: "\n"), !text[..<newline].contains(" ") {
                text = String(text[text.index(after: newline)...]) // drop a language tag like ```text
            }
            text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        }
        return text
    }
}
