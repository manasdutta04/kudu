import Foundation

/// How much the on-device model may change a dictation.
public enum EditingStrength: String, Codable, Sendable, CaseIterable, Identifiable {
    /// Rules only: exactly what you said, with light cleanup.
    case verbatim
    /// Punctuation, paragraphs, lists and number style; your words stay as spoken.
    case light
    /// Also smooths false starts, repetitions and rambling phrasing.
    case polished

    public var id: String { rawValue }

    public var title: String {
        switch self {
        case .verbatim: "Verbatim"
        case .light: "Light"
        case .polished: "Polished"
        }
    }

    public var summary: String {
        switch self {
        case .verbatim: "Exactly what you said, with fillers removed. Fastest."
        case .light: "Adds punctuation, paragraphs, lists and numbers. Keeps your words."
        case .polished: "Also smooths false starts and repetition for cleaner prose."
        }
    }

    /// Largest share of words the model may change.
    public var maximumWordChange: Double { self == .polished ? 0.5 : 0.35 }
    /// Largest change in length the model may make.
    public var maximumLengthChange: Double { self == .polished ? 0.35 : 0.2 }
}

/// Decides when the on-device model is worth its time. The speech model already punctuates
/// and capitalizes ordinary sentences, so the language model is used for what it adds:
/// structure (lists), numbers said in words, and long passages. Greetings and sign-offs are
/// laid out by `MessageLayout`.
/// Everything else is finished instantly by the rules.
public enum FormattingNeed {
    /// From this many words a passage is long enough that paragraphing and run-on fixes pay off.
    public static let longPassageWords = 40

    public static func shouldFormat(_ text: String, strength: EditingStrength) -> Bool {
        guard strength != .verbatim else { return false }
        let words = WordCount.count(text)
        if strength == .polished { return words >= 8 }
        return words >= longPassageWords || (hasStructureCues(text) && words >= 3)
    }

    static let structureCue = try! NSRegularExpression(
        pattern: "\\b(first(ly)?|second(ly)?|third(ly)?|fourth|finally|number (one|two|three)|bullet|step (one|two|three)|next point|"
            + "(one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve|fifteen|twenty|thirty|forty|fifty|sixty|seventy|eighty|ninety|hundred|thousand|million)( |-)?(percent|dollars|rupees|euros|pounds|thousand|million|hundred|o'clock|thirty|fifteen|forty five|am|pm))\\b",
        options: [.caseInsensitive]
    )

    static func hasStructureCues(_ text: String) -> Bool {
        structureCue.firstMatch(in: text, range: NSRange(text.startIndex..., in: text)) != nil
    }
}
