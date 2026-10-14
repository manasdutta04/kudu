import Foundation

/// When the on-device language model formats a dictation.
public enum SmartFormatting: String, Codable, Sendable, CaseIterable, Identifiable {
    case off
    /// Only dictations of `SmartFormatting.minimumWords` or more, so short phrases stay instant.
    case longDictations
    case always

    public var id: String { rawValue }

    public var title: String {
        switch self {
        case .off: "Off"
        case .longDictations: "Longer dictations"
        case .always: "Always"
        }
    }

    public static let minimumWords = 12

    public func applies(toWordCount words: Int) -> Bool {
        switch self {
        case .off: false
        case .longDictations: words >= Self.minimumWords
        case .always: words > 0
        }
    }
}

/// Which cleanup steps run on every dictation. Each one can be turned off in Settings.
public struct CleanupSettings: Codable, Sendable, Equatable {
    public var removeFillers = true
    public var applySelfCorrections = true
    public var spokenCommands = true
    public var normalizeTimes = true
    public var smartFormatting = SmartFormatting.longDictations

    public init() {}

    // Settings files written before a field existed still load, with that field's default.
    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let defaults = CleanupSettings()
        removeFillers = try container.decodeIfPresent(Bool.self, forKey: .removeFillers) ?? defaults.removeFillers
        applySelfCorrections = try container.decodeIfPresent(Bool.self, forKey: .applySelfCorrections) ?? defaults.applySelfCorrections
        spokenCommands = try container.decodeIfPresent(Bool.self, forKey: .spokenCommands) ?? defaults.spokenCommands
        normalizeTimes = try container.decodeIfPresent(Bool.self, forKey: .normalizeTimes) ?? defaults.normalizeTimes
        smartFormatting = try container.decodeIfPresent(SmartFormatting.self, forKey: .smartFormatting) ?? defaults.smartFormatting
    }
}

extension TextPipeline {
    /// The full rule-based pipeline, in the order the stages depend on each other:
    /// snippets match the raw utterance; corrections run before dictionary spelling;
    /// cursor fitting always runs last.
    public static func standard(
        settings: CleanupSettings,
        dictionary: [DictionaryEntry] = [],
        snippets: [Snippet] = [],
        clipboard: @escaping @Sendable () -> String? = { nil }
    ) -> TextPipeline {
        let before = beforeFormatting(settings: settings, dictionary: dictionary, snippets: snippets, clipboard: clipboard)
        return TextPipeline(stages: before.stages + afterFormatting(settings: settings).stages)
    }

    /// Rule cleanup that runs on the raw transcript, before any smart formatting.
    public static func beforeFormatting(
        settings: CleanupSettings,
        dictionary: [DictionaryEntry] = [],
        snippets: [Snippet] = [],
        clipboard: @escaping @Sendable () -> String? = { nil }
    ) -> TextPipeline {
        var stages: [any TextStage] = [WhitespaceNormalizer()]
        if !snippets.isEmpty { stages.append(SnippetStage(snippets: snippets, clipboard: clipboard)) }
        if settings.removeFillers { stages.append(FillerRemover()) }
        if settings.applySelfCorrections { stages.append(SelfCorrection()) }
        if settings.spokenCommands { stages.append(SpokenCommands()) }
        if !dictionary.isEmpty { stages.append(DictionaryStage(entries: dictionary)) }
        return TextPipeline(stages: stages)
    }

    /// Final touches after smart formatting (or straight after rule cleanup).
    public static func afterFormatting(settings: CleanupSettings) -> TextPipeline {
        var stages: [any TextStage] = []
        if settings.normalizeTimes { stages.append(TimeNormalizer()) }
        stages.append(WhitespaceNormalizer())
        stages.append(CursorFitter())
        return TextPipeline(stages: stages)
    }
}
