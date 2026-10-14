import Foundation

/// Which cleanup steps run on every dictation. Each one can be turned off in Settings.
public struct CleanupSettings: Codable, Sendable, Equatable {
    public var removeFillers = true
    public var applySelfCorrections = true
    public var spokenCommands = true
    public var normalizeTimes = true

    public init() {}
}

extension TextPipeline {
    /// The full cleanup pipeline, in the order the stages depend on each other:
    /// snippets match the raw utterance; corrections run before dictionary spelling;
    /// cursor fitting always runs last.
    public static func standard(
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
        if settings.normalizeTimes { stages.append(TimeNormalizer()) }
        if !dictionary.isEmpty { stages.append(DictionaryStage(entries: dictionary)) }
        stages.append(WhitespaceNormalizer())
        stages.append(CursorFitter())
        return TextPipeline(stages: stages)
    }
}
