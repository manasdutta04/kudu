//! Which cleanup steps run on every dictation, and the pipelines built from them.

use crate::editing_strength::EditingStrength;
use crate::pipeline::{TextPipeline, TextStage};
use crate::stages::{
    CursorFitter, DictionaryEntry, DictionaryStage, FileNameStage, FillerRemover, LineStartCapitalizer, MentionStage,
    MessageLayout, Placeholders, SelfCorrection, Snippet, SnippetStage, SpokenCommands, TimeNormalizer,
    WhitespaceNormalizer,
};
use crate::writing_style::{StyleSettings, StyleStage};
use serde::{Deserialize, Serialize};

/// Which cleanup steps run on every dictation. Each one can be turned off in Settings.
///
/// Stored with the macOS app's field names. Settings saved before a field existed still load,
/// with that field's default, and the on/off `smartFormatting` setting of version 0.3 becomes
/// an editing strength.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", from = "StoredCleanupSettings")]
pub struct CleanupSettings {
    pub remove_fillers: bool,
    pub apply_self_corrections: bool,
    pub spoken_commands: bool,
    pub normalize_times: bool,
    pub editing_strength: EditingStrength,
    pub styles: StyleSettings,
}

impl Default for CleanupSettings {
    fn default() -> Self {
        Self {
            remove_fillers: true,
            apply_self_corrections: true,
            spoken_commands: true,
            normalize_times: true,
            editing_strength: EditingStrength::Light,
            styles: StyleSettings::default(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredCleanupSettings {
    remove_fillers: Option<bool>,
    apply_self_corrections: Option<bool>,
    spoken_commands: Option<bool>,
    normalize_times: Option<bool>,
    editing_strength: Option<EditingStrength>,
    /// Read only, for settings saved by 0.3; ignored unless it is a string.
    smart_formatting: Option<serde_json::Value>,
    styles: Option<StyleSettings>,
}

impl From<StoredCleanupSettings> for CleanupSettings {
    fn from(stored: StoredCleanupSettings) -> Self {
        let defaults = Self::default();
        let legacy = stored.smart_formatting.as_ref().and_then(serde_json::Value::as_str).map(|legacy| {
            if legacy == "off" {
                EditingStrength::Verbatim
            } else {
                EditingStrength::Light
            }
        });
        Self {
            remove_fillers: stored.remove_fillers.unwrap_or(defaults.remove_fillers),
            apply_self_corrections: stored.apply_self_corrections.unwrap_or(defaults.apply_self_corrections),
            spoken_commands: stored.spoken_commands.unwrap_or(defaults.spoken_commands),
            normalize_times: stored.normalize_times.unwrap_or(defaults.normalize_times),
            editing_strength: stored.editing_strength.or(legacy).unwrap_or(defaults.editing_strength),
            styles: stored.styles.unwrap_or(defaults.styles),
        }
    }
}

impl TextPipeline {
    /// The full rule-based pipeline, in the order the stages depend on each other: snippets
    /// match the raw utterance; corrections run before dictionary spelling; cursor fitting
    /// always runs last.
    pub fn standard(
        settings: &CleanupSettings,
        dictionary: &[DictionaryEntry],
        snippets: &[Snippet],
        placeholders: Placeholders,
    ) -> Self {
        let mut pipeline = Self::before_formatting(settings, dictionary, snippets, placeholders);
        pipeline.stages.extend(Self::after_formatting(settings).stages);
        pipeline
    }

    /// Rule cleanup that runs on the raw transcript, before any smart formatting.
    pub fn before_formatting(
        settings: &CleanupSettings,
        dictionary: &[DictionaryEntry],
        snippets: &[Snippet],
        placeholders: Placeholders,
    ) -> Self {
        let mut stages: Vec<Box<dyn TextStage>> = vec![Box::new(WhitespaceNormalizer)];
        if !snippets.is_empty() {
            stages.push(Box::new(SnippetStage::new(snippets, placeholders)));
        }
        if settings.remove_fillers {
            stages.push(Box::new(FillerRemover::new()));
        }
        if settings.apply_self_corrections {
            stages.push(Box::new(SelfCorrection));
        }
        if settings.spoken_commands {
            stages.push(Box::new(SpokenCommands));
            stages.push(Box::new(FileNameStage));
        }
        if !dictionary.is_empty() {
            stages.push(Box::new(DictionaryStage::new(dictionary)));
        }
        Self::new(stages)
    }

    /// Final touches after smart formatting (or straight after rule cleanup).
    pub fn after_formatting(settings: &CleanupSettings) -> Self {
        let mut stages: Vec<Box<dyn TextStage>> = Vec::with_capacity(7);
        if settings.normalize_times {
            stages.push(Box::new(TimeNormalizer));
        }
        stages.push(Box::new(WhitespaceNormalizer));
        stages.push(Box::new(MentionStage));
        stages.push(Box::new(MessageLayout));
        stages.push(Box::new(LineStartCapitalizer));
        stages.push(Box::new(StyleStage));
        stages.push(Box::new(CursorFitter));
        Self::new(stages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_category::AppCategory;
    use crate::pipeline::TextContext;
    use crate::writing_style::WritingStyle;

    fn pipeline() -> TextPipeline {
        TextPipeline::standard(
            &CleanupSettings::default(),
            &[DictionaryEntry::new(vec!["kudoo".into()], "Kudu", false)],
            &[Snippet::new("my email", "me@example.com")],
            Placeholders::default(),
        )
    }

    #[test]
    fn end_to_end() {
        let raw = "Um, let's demo kudoo on Tuesday, no, Wednesday at 10.30. New line. Thanks.";
        assert_eq!(pipeline().process(raw, &TextContext::default()), "Let's demo Kudu on Wednesday at 10:30.\nThanks.");
    }

    #[test]
    fn snippet() {
        assert_eq!(pipeline().process("My email.", &TextContext::default()), "me@example.com");
    }

    #[test]
    fn respects_disabled_stages() {
        let settings = CleanupSettings { remove_fillers: false, ..CleanupSettings::default() };
        let pipeline = TextPipeline::standard(&settings, &[], &[], Placeholders::default());
        assert_eq!(pipeline.process("Um, hello.", &TextContext::default()), "Um, hello.");
    }

    #[test]
    fn code_context_continuation() {
        let context = TextContext::new(AppCategory::Code, Some('x'), WritingStyle::Formal);
        assert_eq!(pipeline().process("Uh, Run the tests.", &context), " run the tests");
    }

    #[test]
    fn reads_settings_saved_by_version_0_3() {
        let off: CleanupSettings = serde_json::from_str(r#"{"removeFillers":false,"smartFormatting":"off"}"#).unwrap();
        assert_eq!(off.editing_strength, EditingStrength::Verbatim);
        assert!(!off.remove_fillers);
        let on: CleanupSettings = serde_json::from_str(r#"{"smartFormatting":"longDictations"}"#).unwrap();
        assert_eq!(on.editing_strength, EditingStrength::Light);
        assert_eq!(on.styles, StyleSettings::default());
    }

    #[test]
    fn round_trips() {
        let mut settings =
            CleanupSettings { editing_strength: EditingStrength::Polished, ..CleanupSettings::default() };
        settings.styles.email = WritingStyle::Casual;
        let json = serde_json::to_string(&settings).unwrap();
        let decoded: CleanupSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, settings);
        assert_eq!(
            json,
            r#"{"removeFillers":true,"applySelfCorrections":true,"spokenCommands":true,"normalizeTimes":true,"editingStrength":"polished","styles":{"personalMessages":"casual","workMessages":"casual","email":"casual","other":"formal"}}"#
        );
    }

    #[test]
    fn empty_settings_are_the_defaults() {
        let decoded: CleanupSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(decoded, CleanupSettings::default());
        assert!(serde_json::from_str::<CleanupSettings>(r#"{"editingStrength":"extreme"}"#).is_err());
    }
}
