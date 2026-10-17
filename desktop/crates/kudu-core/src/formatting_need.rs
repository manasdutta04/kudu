//! Decides when the on-device language model is worth its time.
//!
//! The speech model already punctuates and capitalizes ordinary sentences, so the language
//! model is used for what it adds: structure (lists), numbers said in words, and long passages.
//! Greetings and sign-offs are laid out by [`MessageLayout`](crate::stages::MessageLayout).
//! Everything else is finished instantly by the rules.

use crate::app_category::AppCategory;
use crate::editing_strength::EditingStrength;
use crate::usage_stats::word_count;
use regex::Regex;
use std::sync::LazyLock;

/// From this many words a passage is long enough that paragraphing and run-on fixes pay off.
pub const LONG_PASSAGE_WORDS: usize = 40;

static MATH_CUE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(squared|cubed|derivative|integral|square root|to the power|sine|cosine|tangent)\b").unwrap()
});

static CODE_CUE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(dot|dash|underscore|slash|backslash|equals|camel case|snake case|quote|git|npm|yarn|brew|cd|ls|rename|function|variable|method|class)\b",
    )
    .unwrap()
});

static STRUCTURE_CUE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?i)\b(first(ly)?|second(ly)?|third(ly)?|fourth|finally|number (one|two|three)|bullet|step (one|two|three)|next point|",
        r"(two|three|four|five) things|tag [a-z]+|",
        r"squared|cubed|derivative|integral|square root|",
        r"kilos?|kilograms?|grams|litres?|liters?|kilometers?|kilometres?|percent|o'clock|",
        // Dates said as ordinals: "the twelfth of March", "March twenty first".
        r"(fifth|sixth|seventh|eighth|ninth|tenth|eleventh|twelfth|thirteenth|fourteenth|fifteenth|sixteenth|",
        r"seventeenth|eighteenth|nineteenth|twentieth|thirtieth|(twenty|thirty) (first|second|third|fourth|fifth|",
        r"sixth|seventh|eighth|ninth))|",
        // Counts in words: "forty five people".
        r"(twenty|thirty|forty|fifty|sixty|seventy|eighty|ninety)[ -](one|two|three|four|five|six|seven|eight|nine)|",
        r"(one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve|fifteen|twenty|thirty|forty|fifty|sixty|seventy|eighty|ninety|hundred|thousand|million)( |-)?(percent|dollars|rupees|euros|pounds|thousand|million|hundred|o'clock|thirty|fifteen|forty five|am|pm))\b",
    ))
    .unwrap()
});

/// Whether prose should go through the language model.
pub fn should_format(text: &str, strength: EditingStrength, category: AppCategory) -> bool {
    if strength == EditingStrength::Verbatim {
        return false;
    }
    let words = word_count(text);
    if strength == EditingStrength::Polished {
        return words >= 8;
    }
    // An email of a few sentences benefits from paragraphs, greeting and sign-off layout.
    if category == AppCategory::Email && words >= 12 {
        return true;
    }
    words >= LONG_PASSAGE_WORDS || (has_structure_cues(text) && words >= 3)
}

/// Spoken math ("x squared", "the derivative of"), which the model turns into notation.
pub fn has_math_cues(text: &str) -> bool {
    MATH_CUE.is_match(text)
}

/// In code editors and terminals the model is used when the dictation names code (spoken
/// symbols, commands, identifiers to rename) or is a longer instruction.
pub fn should_format_code(text: &str, strength: EditingStrength) -> bool {
    if strength == EditingStrength::Verbatim {
        return false;
    }
    CODE_CUE.is_match(text) || word_count(text) >= 10
}

pub(crate) fn has_structure_cues(text: &str) -> bool {
    STRUCTURE_CUE.is_match(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use EditingStrength::*;

    fn should(text: &str, strength: EditingStrength) -> bool {
        should_format(text, strength, AppCategory::General)
    }

    #[test]
    fn plain_sentences_skip_the_model() {
        assert!(!should("Can we move the stand-up to Wednesday?", Light));
        assert!(!should("Hey team, the build is green and I pushed the fix for the login bug.", Light));
    }

    #[test]
    fn structure_or_length_uses_it() {
        assert!(should("first the api second the tests", Light));
        assert!(should("budget is twelve thousand dollars", Light));
        let long = vec!["word"; 45].join(" ");
        assert!(should(&long, Light));
    }

    #[test]
    fn verbatim_never_uses_it_and_polished_uses_it_more() {
        assert!(!should("first the api second the tests and third the release notes", Verbatim));
        assert!(should("so i think we should probably maybe ship it", Polished));
    }

    #[test]
    fn code_cues_and_prose_cues() {
        assert!(should_format_code("git commit dash m fixed it", Light));
        assert!(!should_format_code("yes", Light));
        assert!(should("add two kilo atta", Light));
        assert!(should("quick update three things first design", Light));
    }

    #[test]
    fn units_dates_and_counts_in_words_use_it() {
        assert!(should("the package weighs three kilograms", Light));
        assert!(should("we walked five kilometers today", Light));
        assert!(should("we need two litres of milk", Light));
        assert!(should("the launch is on the twelfth of March", Light));
        assert!(should("see you on March twenty first", Light));
        assert!(should("forty five people signed up", Light));
        assert!(should("meet me at eight o'clock", Light));
        assert!(!should("the fifth wheel of the team", Verbatim));
    }

    #[test]
    fn email_formats_a_few_sentences() {
        let text = "hi sam the deck is attached let me know what you think thanks manas";
        assert!(should_format(text, Light, AppCategory::Email));
        assert!(!should_format(text, Light, AppCategory::WorkMessage));
        assert!(has_math_cues("x squared plus y"));
    }
}
