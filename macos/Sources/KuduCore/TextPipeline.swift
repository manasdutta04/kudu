import Foundation

/// Broad class of the app receiving the text; drives formatting decisions.
public enum AppCategory: String, Sendable, Codable, CaseIterable {
    case general
    case code      // editors, IDEs, terminals: no forced capitalization or trailing period
    case chat      // Slack, Messages, Discord: casual
    case email     // Mail, Outlook: formal
}

/// What we know about the insertion point when cleaning up text.
public struct TextContext: Sendable, Equatable {
    public var category: AppCategory
    /// The character immediately before the cursor, if it could be read via Accessibility.
    public var precedingCharacter: Character?

    public init(category: AppCategory = .general, precedingCharacter: Character? = nil) {
        self.category = category
        self.precedingCharacter = precedingCharacter
    }
}

/// One pure transformation step applied to a raw transcript.
public protocol TextStage: Sendable {
    func apply(_ text: String, context: TextContext) -> String
}

/// An ordered list of stages. Every stage is a pure function, so the pipeline is
/// cheap (well under a millisecond) and trivially testable.
public struct TextPipeline: Sendable {
    public var stages: [any TextStage]

    public init(stages: [any TextStage]) {
        self.stages = stages
    }

    public func process(_ text: String, context: TextContext = TextContext()) -> String {
        stages.reduce(text) { partial, stage in stage.apply(partial, context: context) }
    }

    /// Minimal pipeline for M1: tidy whitespace and fit the text to the cursor position.
    public static let basic = TextPipeline(stages: [
        WhitespaceNormalizer(),
        CursorFitter(),
    ])
}
