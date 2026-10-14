use crate::pipeline::{TextContext, TextStage};
use crate::string_casing::capitalizing_sentence_starts;
use crate::text::{is_punctuation, trim_spaces};
use regex::Regex;
use std::sync::LazyLock;

/// Hesitation sounds removed by default.
pub const DEFAULT_FILLERS: &[&str] = &["um", "umm", "uh", "uhh", "uhm", "erm", "er", "hmm", "mm"];

static DEFAULT_REGEX: LazyLock<Regex> = LazyLock::new(|| filler_regex(DEFAULT_FILLERS));
static DOUBLE_COMMA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r",\s*,").unwrap());

/// A filler is a whole word, optionally followed by "," "." or "…", plus the space after it.
fn filler_regex<S: AsRef<str>>(fillers: &[S]) -> Regex {
    let alternatives = fillers.iter().map(|filler| regex::escape(filler.as_ref())).collect::<Vec<_>>().join("|");
    Regex::new(&format!(r"(?i)\b(?:{alternatives})\b[,.…]*\s*")).unwrap()
}

/// Removes hesitation sounds ("um", "uh", "erm", "hmm") along with the comma or period the
/// speech model attaches to them, and re-capitalizes a sentence that started with one.
#[derive(Clone, Debug)]
pub struct FillerRemover {
    regex: Regex,
}

impl FillerRemover {
    /// Removes [`DEFAULT_FILLERS`].
    pub fn new() -> Self {
        Self { regex: DEFAULT_REGEX.clone() }
    }

    pub fn with_fillers<S: AsRef<str>>(fillers: &[S]) -> Self {
        Self { regex: filler_regex(fillers) }
    }
}

impl Default for FillerRemover {
    fn default() -> Self {
        Self::new()
    }
}

impl TextStage for FillerRemover {
    fn apply(&self, text: &str, _context: &TextContext) -> String {
        if !self.regex.is_match(text) {
            return text.to_owned();
        }
        let result = self.regex.replace_all(text, "");
        // "So, , we" → "So, we" when a filler sat between two commas.
        let result = DOUBLE_COMMA.replace_all(&result, ",");
        let mut result = trim_spaces(&result).to_owned();
        if result.ends_with(',') {
            result.pop();
        }
        // "Ship it, uh." loses its final period along with the filler; put it back.
        if let Some(terminal) = text.chars().next_back().filter(|c| matches!(c, '.' | '?' | '!')) {
            if result.chars().next_back().is_some_and(|last| !is_punctuation(last)) {
                result.push(terminal);
            }
        }
        capitalizing_sentence_starts(&result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_fillers() {
        let cases = [
            ("Um, so we should ship it.", "So we should ship it."),
            ("So, uh, we should ship it.", "So, we should ship it."),
            ("I think, umm, Tuesday works.", "I think, Tuesday works."),
            ("Hmm. Let me check.", "Let me check."),
            ("Ship it, uh.", "Ship it."),
        ];
        for (input, expected) in cases {
            assert_eq!(FillerRemover::new().apply(input, &TextContext::default()), expected, "{input}");
        }
    }

    #[test]
    fn leaves_words_containing_fillers_alone() {
        let text = "The umbrella is under the drum.";
        assert_eq!(FillerRemover::new().apply(text, &TextContext::default()), text);
    }

    #[test]
    fn leaves_clean_text_untouched() {
        let text = "already clean text";
        assert_eq!(FillerRemover::new().apply(text, &TextContext::default()), text);
    }

    #[test]
    fn custom_fillers() {
        let stage = FillerRemover::with_fillers(&["like"]);
        assert_eq!(stage.apply("It was, like, great.", &TextContext::default()), "It was, great.");
    }
}
