use crate::pipeline::{TextContext, TextStage};
use crate::string_casing::capitalizing_sentence_starts;
use regex::{NoExpand, Regex};
use std::sync::LazyLock;

/// Each command, then its replacement. Optional trailing comma before the command (the
/// preceding sentence keeps its own period), the command itself, then any punctuation and
/// spaces after it.
static COMMANDS: LazyLock<[(Regex, &str); 3]> = LazyLock::new(|| {
    let command = |words: &str| Regex::new(&format!(r"(?i),?\s*\b{words}\b[,.!?]*\s*")).unwrap();
    [(command("new paragraph"), "\n\n"), (command("new line"), "\n"), (command("next line"), "\n")]
});

/// Turns spoken layout commands into formatting: "new line" → line break, "new paragraph" →
/// blank line. The speech model punctuates these like words ("Thanks. New paragraph. Best,"),
/// so the surrounding punctuation is absorbed too.
#[derive(Clone, Copy, Debug, Default)]
pub struct SpokenCommands;

impl TextStage for SpokenCommands {
    fn apply(&self, text: &str, _context: &TextContext) -> String {
        let mut result = text.to_owned();
        let mut changed = false;
        for (regex, replacement) in COMMANDS.iter() {
            if !regex.is_match(&result) {
                continue;
            }
            result = regex.replace_all(&result, NoExpand(replacement)).into_owned();
            changed = true;
        }
        if !changed {
            return result;
        }
        // Text after a break starts a new sentence.
        capitalizing_sentence_starts(&result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(text: &str) -> String {
        SpokenCommands.apply(text, &TextContext::default())
    }

    #[test]
    fn new_line() {
        assert_eq!(apply("Hi Sam. New line. Are you free?"), "Hi Sam.\nAre you free?");
    }

    #[test]
    fn new_paragraph() {
        assert_eq!(apply("Thanks for the update, new paragraph, best, Manas"), "Thanks for the update\n\nBest, Manas");
    }

    #[test]
    fn lowercase_following_text_is_capitalized() {
        assert_eq!(apply("first item new line second item"), "First item\nSecond item");
    }

    #[test]
    fn ordinary_text_is_untouched() {
        let text = "We need a newline character here.";
        assert_eq!(apply(text), text);
    }
}
