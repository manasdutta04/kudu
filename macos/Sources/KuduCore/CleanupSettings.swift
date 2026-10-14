import Foundation

/// Which cleanup steps run on every dictation. Each one can be turned off in Settings.
public struct CleanupSettings: Codable, Sendable, Equatable {
    public var removeFillers = true
    public var applySelfCorrections = true
    public var spokenCommands = true
    public var normalizeTimes = true
    public var editingStrength = EditingStrength.light
    public var styles = StyleSettings()

    public init() {}

    // Settings files written before a field existed still load, with that field's default.
    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let defaults = CleanupSettings()
        removeFillers = try container.decodeIfPresent(Bool.self, forKey: .removeFillers) ?? defaults.removeFillers
        applySelfCorrections = try container.decodeIfPresent(Bool.self, forKey: .applySelfCorrections) ?? defaults.applySelfCorrections
        spokenCommands = try container.decodeIfPresent(Bool.self, forKey: .spokenCommands) ?? defaults.spokenCommands
        normalizeTimes = try container.decodeIfPresent(Bool.self, forKey: .normalizeTimes) ?? defaults.normalizeTimes
        if let strength = try container.decodeIfPresent(EditingStrength.self, forKey: .editingStrength) {
            editingStrength = strength
        } else if let legacy = try? container.decodeIfPresent(String.self, forKey: .smartFormatting) {
            // 0.3 stored an on/off-style "smartFormatting" setting.
            editingStrength = legacy == "off" ? .verbatim : .light
        }
        styles = try container.decodeIfPresent(StyleSettings.self, forKey: .styles) ?? defaults.styles
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(removeFillers, forKey: .removeFillers)
        try container.encode(applySelfCorrections, forKey: .applySelfCorrections)
        try container.encode(spokenCommands, forKey: .spokenCommands)
        try container.encode(normalizeTimes, forKey: .normalizeTimes)
        try container.encode(editingStrength, forKey: .editingStrength)
        try container.encode(styles, forKey: .styles)
    }

    private enum CodingKeys: String, CodingKey {
        case removeFillers, applySelfCorrections, spokenCommands, normalizeTimes, editingStrength, styles
        case smartFormatting // read-only, for settings saved by 0.3
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
        if settings.spokenCommands { stages.append(contentsOf: [SpokenCommands(), FileNameStage()] as [any TextStage]) }
        if !dictionary.isEmpty { stages.append(DictionaryStage(entries: dictionary)) }
        return TextPipeline(stages: stages)
    }

    /// Final touches after smart formatting (or straight after rule cleanup).
    public static func afterFormatting(settings: CleanupSettings) -> TextPipeline {
        var stages: [any TextStage] = []
        if settings.normalizeTimes { stages.append(TimeNormalizer()) }
        stages.append(WhitespaceNormalizer())
        stages.append(MentionStage())
        stages.append(MessageLayout())
        stages.append(LineStartCapitalizer())
        stages.append(StyleStage())
        stages.append(CursorFitter())
        return TextPipeline(stages: stages)
    }
}
