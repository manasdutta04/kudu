//! The text pipeline: an ordered list of pure stages that turn a raw transcript into the text
//! that is inserted.

use crate::app_category::AppCategory;
use crate::stages::{CursorFitter, WhitespaceNormalizer};
use crate::writing_style::WritingStyle;

/// What we know about the insertion point when cleaning up text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextContext {
    pub category: AppCategory,
    /// The character immediately before the cursor, if it could be read.
    pub preceding_character: Option<char>,
    /// How the user wants text written in this category of app.
    pub style: WritingStyle,
}

impl TextContext {
    pub fn new(category: AppCategory, preceding_character: Option<char>, style: WritingStyle) -> Self {
        Self { category, preceding_character, style }
    }

    pub fn for_category(category: AppCategory) -> Self {
        Self { category, ..Self::default() }
    }
}

impl Default for TextContext {
    fn default() -> Self {
        Self { category: AppCategory::General, preceding_character: None, style: WritingStyle::Formal }
    }
}

/// One pure transformation step applied to a transcript.
pub trait TextStage: Send + Sync {
    fn apply(&self, text: &str, context: &TextContext) -> String;
}

/// An ordered list of stages. Every stage is a pure function, so the pipeline is cheap (well
/// under a millisecond) and trivially testable. Build it once per settings change and reuse
/// it: stages compile their patterns when they are created.
#[derive(Default)]
pub struct TextPipeline {
    pub stages: Vec<Box<dyn TextStage>>,
}

impl TextPipeline {
    pub fn new(stages: Vec<Box<dyn TextStage>>) -> Self {
        Self { stages }
    }

    pub fn process(&self, text: &str, context: &TextContext) -> String {
        let mut result = text.to_owned();
        for stage in &self.stages {
            result = stage.apply(&result, context);
        }
        result
    }

    /// Minimal pipeline: tidy whitespace and fit the text to the cursor position.
    pub fn basic() -> Self {
        Self::new(vec![Box::new(WhitespaceNormalizer), Box::new(CursorFitter)])
    }
}

impl std::fmt::Debug for TextPipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TextPipeline({} stages)", self.stages.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_pipeline_runs_all_stages() {
        let context = TextContext::new(AppCategory::Code, Some('x'), WritingStyle::Formal);
        assert_eq!(TextPipeline::basic().process("  Run the tests .  ", &context), " run the tests");
    }
}
