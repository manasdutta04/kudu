import Foundation

/// File names said aloud are written as file names: "fix the use case dot ts" → "fix the
/// useCase.ts", "open read me dot md" → "open README.md", "package dot json" → "package.json".
///
/// The name is the one to three words before "dot", stopping at a verb or small word
/// ("open", "the", "in"), joined in camelCase the way code names files.
public struct FileNameStage: TextStage {
    static let extensions = [
        "ts", "tsx", "js", "jsx", "mjs", "py", "swift", "json", "md", "mdx", "rs", "go", "java", "kt", "rb", "css", "scss",
        "html", "yml", "yaml", "sh", "txt", "cpp", "c", "h", "vue", "php", "sql", "toml", "env", "csv", "pdf", "png",
        "jpg", "svg", "xml", "lock", "dart", "cs", "ex", "zig", "lua", "ini", "plist", "docx", "xlsx",
    ]

    private static let pattern = try! NSRegularExpression(
        pattern: "((?:\\b[A-Za-z][A-Za-z0-9'’]*[ ]+){1,4})dot (\(extensions.joined(separator: "|")))\\b",
        options: [.caseInsensitive])

    /// Words that end a file name going backwards: verbs and small words before it.
    private static let boundary: Set<String> = CorrectionLearner.stopwords.union([
        "open", "opened", "fix", "fixed", "edit", "check", "update", "updated", "see", "run", "delete", "remove",
        "rename", "create", "created", "move", "save", "look", "change", "changed", "add", "added", "test", "review",
        "push", "pushed", "commit", "close", "import", "copy", "send", "share", "upload", "download", "attach",
        "attached", "write", "modify", "refactor", "inside", "file", "called", "named", "new", "old", "under",
        "go", "please", "read", "using",
    ])

    /// Names that are conventionally written in capitals.
    private static let conventional: [String: String] = [
        "readme": "README", "license": "LICENSE", "changelog": "CHANGELOG", "contributing": "CONTRIBUTING",
        "dockerfile": "Dockerfile", "makefile": "Makefile",
    ]

    public init() {}

    public func apply(_ text: String, context: TextContext) -> String {
        var result = text
        for match in Self.pattern.matches(in: text, range: NSRange(text.startIndex..., in: text)).reversed() {
            guard let whole = Range(match.range, in: result),
                  let wordsRange = Range(match.range(at: 1), in: result),
                  let extRange = Range(match.range(at: 2), in: result)
            else { continue }
            let candidates = result[wordsRange].split(separator: " ").map(String.init)
            // Walk back from "dot" while the words look like part of the name.
            var name: [String] = []
            for word in candidates.reversed() {
                let lower = word.lowercased()
                // "read me" is a name, even though "read" is a verb.
                if lower == "read", name.first?.lowercased() == "me" {
                    name.insert(word, at: 0)
                    break
                }
                if Self.boundary.contains(lower) && !(lower == "me" && name.isEmpty) { break }
                name.insert(word, at: 0)
                if name.count == 3 { break }
            }
            guard !name.isEmpty else { continue }
            let file = Self.join(name) + "." + result[extRange].lowercased()
            // Replace from the first name word to the extension.
            let nameText = name.joined(separator: " ")
            guard let nameStart = result[whole].range(of: nameText, options: .backwards)?.lowerBound,
                  nameStart >= whole.lowerBound
            else { continue }
            result.replaceSubrange(nameStart..<whole.upperBound, with: file)
        }
        return result
    }

    static func join(_ words: [String]) -> String {
        let joined = words.map { $0.lowercased() }.joined()
        if let conventional = conventional[joined] { return conventional }
        if words.count == 1 { return words[0] }
        return words.enumerated().map { index, word in
            if index == 0 { return word.prefix(1).lowercased() + word.dropFirst() }
            return word.prefix(1).uppercased() + word.dropFirst()
        }.joined()
    }
}
