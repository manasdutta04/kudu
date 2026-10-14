use crate::pipeline::{TextContext, TextStage};
use crate::text::fancy_replace_all;
use regex::Regex;
use std::sync::LazyLock;

static AFTER_PREPOSITION: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
    fancy_regex::Regex::new(
        r"(?i)\b(at|by|until|till|from|to|around|before|after|between|and)(\s+)([01]?\d|2[0-3])\.([0-5]\d)\b(?!\.\d)",
    )
    .unwrap()
});

static BEFORE_MERIDIEM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(1[0-2]|0?[1-9])\.([0-5]\d)(\s*)(am|pm|a\.m\.|p\.m\.)").unwrap());

/// The speech model writes clock times with a dot ("at 10.30"). Rewrites them as "10:30" when
/// the context makes it clearly a time: a preceding "at/by/until/from/to/around/before/after",
/// or a following "am/pm". Version numbers and decimals ("version 2.10", "3.14") are untouched.
#[derive(Clone, Copy, Debug, Default)]
pub struct TimeNormalizer;

impl TextStage for TimeNormalizer {
    fn apply(&self, text: &str, _context: &TextContext) -> String {
        let result = BEFORE_MERIDIEM.replace_all(text, "${1}:${2}${3}${4}");
        fancy_replace_all(&AFTER_PREPOSITION, &result, "${1}${2}${3}:${4}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_times() {
        let cases = [
            ("Can we move the stand-up to Wednesday at 10.30?", "Can we move the stand-up to Wednesday at 10:30?"),
            ("Lunch is 12.15 pm.", "Lunch is 12:15 pm."),
            ("Free from 9.00 to 11.45.", "Free from 9:00 to 11:45."),
            ("Call me around 7.05 a.m.", "Call me around 7:05 a.m."),
        ];
        for (input, expected) in cases {
            assert_eq!(TimeNormalizer.apply(input, &TextContext::default()), expected, "{input}");
        }
    }

    #[test]
    fn leaves_numbers_alone() {
        for text in [
            "Upgrade to version 2.10.4 today.",
            "Pi is roughly 3.14.",
            "Revenue grew 12.50 percent.",
            "We hit 24.99 dollars.",
        ] {
            assert_eq!(TimeNormalizer.apply(text, &TextContext::default()), text);
        }
    }
}
