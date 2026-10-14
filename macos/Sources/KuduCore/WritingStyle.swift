import Foundation

/// How formal the text should read, chosen per category of app (Style page).
public enum WritingStyle: String, Codable, Sendable, CaseIterable, Identifiable {
    /// Full sentences with capitals and punctuation.
    case formal
    /// Normal capitals; short messages drop their final period.
    case casual
    /// All lowercase, no final period.
    case veryCasual

    public var id: String { rawValue }

    public var title: String {
        switch self {
        case .formal: "Formal"
        case .casual: "Casual"
        case .veryCasual: "Very casual"
        }
    }

    public var example: String {
        switch self {
        case .formal: "Sounds good. See you at 3."
        case .casual: "Sounds good. See you at 3"
        case .veryCasual: "sounds good. see you at 3"
        }
    }
}

/// The user's style for each category of app.
public struct StyleSettings: Codable, Sendable, Equatable {
    public var personalMessages = WritingStyle.casual
    public var workMessages = WritingStyle.casual
    public var email = WritingStyle.formal
    public var other = WritingStyle.formal

    public init() {}

    public func style(for category: AppCategory) -> WritingStyle {
        switch category {
        case .personalMessage: personalMessages
        case .workMessage: workMessages
        case .email: email
        case .general, .code: other
        }
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let defaults = StyleSettings()
        personalMessages = try container.decodeIfPresent(WritingStyle.self, forKey: .personalMessages) ?? defaults.personalMessages
        workMessages = try container.decodeIfPresent(WritingStyle.self, forKey: .workMessages) ?? defaults.workMessages
        email = try container.decodeIfPresent(WritingStyle.self, forKey: .email) ?? defaults.email
        other = try container.decodeIfPresent(WritingStyle.self, forKey: .other) ?? defaults.other
    }
}

/// Applies the writing style from the context. Runs after formatting, before cursor fitting.
public struct StyleStage: TextStage {
    /// Messages up to this many words count as "short" for dropping the final period.
    public static let shortMessageWords = 30

    public init() {}

    public func apply(_ text: String, context: TextContext) -> String {
        guard context.category != .code, !text.isEmpty else { return text }
        switch context.style {
        case .formal:
            return text
        case .casual:
            guard context.category.isMessage, WordCount.count(text) <= Self.shortMessageWords else { return text }
            return Self.droppingFinalPeriod(text)
        case .veryCasual:
            return Self.droppingFinalPeriod(Self.lowercasingSentenceStarts(text))
        }
    }

    /// "See you soon." → "See you soon". Keeps "?", "!" and "...".
    static func droppingFinalPeriod(_ text: String) -> String {
        guard text.hasSuffix("."), !text.hasSuffix("..") else { return text }
        return String(text.dropLast())
    }

    /// Lowercases the first letter of each sentence, but not "I", acronyms ("API") or words
    /// with capitals inside ("GitHub"), which are names.
    static func lowercasingSentenceStarts(_ text: String) -> String {
        var result = ""
        var atStart = true
        var index = text.startIndex
        while index < text.endIndex {
            let character = text[index]
            if atStart, character.isLetter {
                let word = text[index...].prefix { $0.isLetter || $0 == "'" || $0 == "’" }
                let keep = word == "I" || word.hasPrefix("I'") || word.hasPrefix("I’")
                    || word.dropFirst().contains(where: \.isUppercase)
                result.append(keep ? String(character) : character.lowercased())
                atStart = false
            } else {
                result.append(character)
                if ".?!\n".contains(character) {
                    atStart = true
                } else if !character.isWhitespace {
                    atStart = false
                }
            }
            index = text.index(after: index)
        }
        return result
    }
}
