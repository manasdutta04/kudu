use crate::app_category::AppCategory;
use crate::pipeline::{TextContext, TextStage};
use crate::text::{is_punctuation, lower_first};

/// Makes the inserted text read naturally where the cursor is:
/// - adds a leading space when continuing after a word,
/// - lowercases the first letter when continuing mid-sentence,
/// - in code contexts, drops the trailing period speech models add to every utterance.
#[derive(Clone, Copy, Debug, Default)]
pub struct CursorFitter;

impl TextStage for CursorFitter {
    fn apply(&self, text: &str, context: &TextContext) -> String {
        if text.is_empty() {
            return String::new();
        }
        let mut result = text.to_owned();

        if context.category == AppCategory::Code && result.ends_with('.') && !result.ends_with("..") {
            result.pop();
        }

        let Some(previous) = context.preceding_character else {
            return result;
        };

        let continues_sentence = previous.is_alphabetic() || previous.is_numeric() || previous == ',';
        if continues_sentence && result.chars().next().is_some_and(char::is_uppercase) && !keeps_capital(&result) {
            result = lower_first(&result);
        }
        if !previous.is_whitespace()
            && !matches!(previous, '(' | '[' | '"')
            && result.chars().next().is_some_and(|first| !is_punctuation(first))
        {
            result.insert(0, ' ');
        }
        result
    }
}

/// "I", "I'm" and acronyms keep their capitals mid-sentence.
fn keeps_capital(text: &str) -> bool {
    let end = text.find(|c: char| !(c.is_alphabetic() || c == '\'')).unwrap_or(text.len());
    let first_word = &text[..end];
    if first_word == "I" || first_word.starts_with("I'") {
        return true;
    }
    first_word.chars().count() > 1 && first_word.chars().all(char::is_uppercase)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writing_style::WritingStyle;

    fn after(previous: char) -> TextContext {
        TextContext::new(AppCategory::General, Some(previous), WritingStyle::Formal)
    }

    #[test]
    fn no_context_leaves_text_alone() {
        assert_eq!(CursorFitter.apply("Hello there.", &TextContext::default()), "Hello there.");
    }

    #[test]
    fn continues_after_word() {
        assert_eq!(CursorFitter.apply("And then more.", &after('d')), " and then more.");
    }

    #[test]
    fn keeps_pronoun_i_capitalized() {
        assert_eq!(CursorFitter.apply("I think so.", &after(',')), " I think so.");
    }

    #[test]
    fn keeps_acronyms() {
        assert_eq!(CursorFitter.apply("NASA launched.", &after('e')), " NASA launched.");
    }

    #[test]
    fn no_space_after_whitespace_or_open_paren() {
        assert_eq!(CursorFitter.apply("Yes.", &after(' ')), "Yes.");
        assert_eq!(CursorFitter.apply("Yes.", &after('(')), "Yes.");
    }

    #[test]
    fn new_sentence_keeps_capital() {
        assert_eq!(CursorFitter.apply("Next one.", &after('.')), " Next one.");
    }

    #[test]
    fn code_context_drops_trailing_period() {
        let context = TextContext::for_category(AppCategory::Code);
        assert_eq!(CursorFitter.apply("git status.", &context), "git status");
        assert_eq!(CursorFitter.apply("wait...", &context), "wait...");
    }
}
