use crate::pipeline::{TextContext, TextStage};
use regex::Regex;
use std::sync::LazyLock;

static SPACE_RUNS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t]{2,}").unwrap());
static SPACE_BEFORE_PUNCTUATION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r" +([,.;:!?])").unwrap());
static SPACE_BEFORE_LINE_BREAK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t]+\n").unwrap());
static SPACE_AFTER_LINE_BREAK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n[ \t]+").unwrap());

/// Collapses runs of spaces, removes spaces before punctuation, and trims the ends.
#[derive(Clone, Copy, Debug, Default)]
pub struct WhitespaceNormalizer;

impl TextStage for WhitespaceNormalizer {
    fn apply(&self, text: &str, _context: &TextContext) -> String {
        let result = SPACE_RUNS.replace_all(text, " ");
        let result = SPACE_BEFORE_PUNCTUATION.replace_all(&result, "${1}");
        let result = SPACE_BEFORE_LINE_BREAK.replace_all(&result, "\n");
        let result = SPACE_AFTER_LINE_BREAK.replace_all(&result, "\n");
        result.trim().to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes() {
        let cases = [("  hello   world  ", "hello world"), ("wait , what ?", "wait, what?"), ("one\ttwo", "one\ttwo")];
        for (input, expected) in cases {
            assert_eq!(WhitespaceNormalizer.apply(input, &TextContext::default()), expected, "{input:?}");
        }
    }
}
