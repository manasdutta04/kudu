use crate::pipeline::{TextContext, TextStage};
use crate::string_casing::capitalizing_sentence_starts;
use crate::text::{fancy_replace_all, trim_spaces};
use regex::Regex;
use std::sync::LazyLock;

// <word>, no, | <word>, sorry, | <word>, no wait, | <word>, wait, no,
// "no wait" and "or rather" are unambiguous enough to count even without commas.
static INLINE_CUE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b[\w'’]+(?:,\s+(?:no|sorry|no wait|wait,? no|or rather),|,?\s+(?:no wait|wait no|or rather),?)\s+",
    )
    .unwrap()
});

// A sentence (or the start of the text) followed by "scratch that" / "delete that".
static SCRATCH_THAT: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
    fancy_regex::Regex::new(r"(?i)(?:^|(?<=[.?!]\s))[^.?!]*[.?!,]?\s*\b(?:scratch that|delete that)\b[.,!]?\s*")
        .unwrap()
});

/// Applies the corrections people make out loud while dictating:
/// - "Let's meet Tuesday, no, Wednesday" → "Let's meet Wednesday"
/// - "at 3, sorry, 4 pm" → "at 4 pm"
/// - "Send it to Alex. Scratch that. Send it to Sam." → "Send it to Sam."
///
/// Deliberately conservative: a correction cue only counts when the speech model has set it
/// off with commas (or as its own sentence for "scratch that"), and only the single word
/// before the cue is replaced. Ordinary uses of "no" or "sorry" are left alone.
#[derive(Clone, Copy, Debug, Default)]
pub struct SelfCorrection;

impl TextStage for SelfCorrection {
    fn apply(&self, text: &str, _context: &TextContext) -> String {
        let result = fancy_replace_all(&SCRATCH_THAT, text, "");
        let result = INLINE_CUE.replace_all(&result, "");
        if result == text {
            return text.to_owned();
        }
        capitalizing_sentence_starts(trim_spaces(&result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_corrections() {
        let cases = [
            ("Let's meet Tuesday, no, Wednesday.", "Let's meet Wednesday."),
            ("Call me at 3, sorry, 4 pm.", "Call me at 4 pm."),
            ("Book the red, no wait, blue room.", "Book the blue room."),
            ("Send it to Alex. Scratch that. Send it to Sam.", "Send it to Sam."),
            ("Scratch that, ship it today.", "Ship it today."),
        ];
        for (input, expected) in cases {
            assert_eq!(SelfCorrection.apply(input, &TextContext::default()), expected, "{input}");
        }
    }

    #[test]
    fn leaves_ordinary_uses_alone() {
        for text in ["No, I don't think so.", "Sorry, I missed that.", "There is no way, sorry.", "Say no to meetings."]
        {
            assert_eq!(SelfCorrection.apply(text, &TextContext::default()), text);
        }
    }
}
