//! Cleans what the language model returns in Command mode so only the text to insert is left:
//! no "Sure! Here's the revised text:" preamble, no wrapping quotes or code fences, no
//! "Let me know if…" afterword.

use crate::text::{char_count, is_punctuation, lowercase, upper_first, ICU_DOT};
use regex::Regex;
use std::sync::LazyLock;

static PREAMBLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^(sure|certainly|of course|absolutely|okay|ok|here(’|')?s|here is|here are|below is)\b[^\n]*?:\s*\n?",
    )
    .unwrap()
});

/// "Dear, may I…": a salutation with no name, which the model sometimes adds when it
/// formalizes "hey". Group 1 is the lowercase letter that must follow (ICU's look-ahead).
static NAMELESS_SALUTATION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(?i:dear),\s+(\p{Ll})").unwrap());

static AFTERWORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\n+\s*(let me know|i hope this|feel free to|is there anything)[^\n]*$").unwrap());

/// Lines where the model repeated its prompt instead of answering.
static ECHO: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?imR)^\s*(instruction|request)\s*:{ICU_DOT}*$|^\s*(write the text now|apply it to this text|return only)\b{ICU_DOT}*$"
    ))
    .unwrap()
});

/// Instructions that edit existing text ("fix this", "make it shorter", "translate that to
/// Hindi"), which need a selection, as opposed to drafting ("write a reply saying…").
static EDITS_EXISTING_TEXT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)^(please )?(fix|rewrite|rephrase|reword|translate|shorten|summari[sz]e|make|improve|proofread|correct|format|simplify|expand|polish|edit|change|convert|turn|clean up|tidy|check)\b{ICU_DOT}*\b(this|it|that|these|those|them|the text|the selection|the above|the grammar|the spelling|the tone|the wording)\b"
    ))
    .unwrap()
});

fn trim_whitespace_and_punctuation(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_whitespace() || is_punctuation(c))
}

/// Whether a Command mode instruction edits selected text rather than drafting new text.
pub fn needs_selection(instruction: &str) -> bool {
    EDITS_EXISTING_TEXT.is_match(trim_whitespace_and_punctuation(instruction))
}

/// The text to insert from a Command mode answer. With the `instruction`, an answer that
/// only repeats it comes back empty.
pub fn clean(output: &str, instruction: Option<&str>) -> String {
    let text = ECHO.replace_all(output.trim(), "");
    if let Some(instruction) = instruction {
        if lowercase(trim_whitespace_and_punctuation(&text)) == lowercase(trim_whitespace_and_punctuation(instruction))
        {
            return String::new(); // just the instruction said back
        }
    }
    let text = PREAMBLE.replace_all(text.trim(), "");
    let mut text = AFTERWORD.replace_all(&text, "").into_owned();
    if let Some(letter) = NAMELESS_SALUTATION.captures(&text).and_then(|captures| captures.get(1)) {
        text = upper_first(&text[letter.start()..]);
    }
    let mut text = text.trim();
    for (open, close) in [("```", "```"), ("\"", "\""), ("“", "”")] {
        if text.starts_with(open) && text.ends_with(close) && char_count(text) > char_count(open) + char_count(close) {
            text = &text[open.len()..text.len() - close.len()];
            if open == "```" {
                if let Some(newline) = text.find('\n') {
                    if !text[..newline].contains(' ') {
                        text = &text[newline + 1..]; // drop a language tag like ```text
                    }
                }
            }
            text = text.trim();
        }
    }
    text.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips() {
        let cases = [
            ("Sure! Here's a more formal version:\nI will be late to the meeting.", "I will be late to the meeting."),
            ("Here is the translation: नमस्ते, आप कैसे हैं?", "नमस्ते, आप कैसे हैं?"),
            ("\"Thanks for the update.\"", "Thanks for the update."),
            ("```\nlet x = 1\n```", "let x = 1"),
            ("Thanks, I'll review it today.\n\nLet me know if you want any changes!", "Thanks, I'll review it today."),
            ("Dear, may I request the deck by tomorrow?", "May I request the deck by tomorrow?"),
        ];
        for (output, expected) in cases {
            assert_eq!(clean(output, None), expected, "{output:?}");
        }
    }

    #[test]
    fn leaves_plain_text_alone() {
        for text in [
            "Here we go again: the launch slipped to Friday.",
            "Ok so the launch slipped to Friday.",
            "Dear Priya, thanks for the update.",
        ] {
            assert_eq!(clean(text, None), text);
        }
    }

    #[test]
    fn editing_instructions_need_a_selection() {
        assert!(needs_selection("Fix this"));
        assert!(needs_selection("make it more formal"));
        assert!(needs_selection("Translate that to Hindi."));
        assert!(needs_selection("fix the grammar"));
    }

    #[test]
    fn drafts_do_not() {
        assert!(!needs_selection("write a polite reply saying I'll be late"));
        assert!(!needs_selection("translate good morning to Hindi"));
        assert!(!needs_selection("draft a tweet about our launch"));
    }

    #[test]
    fn drops_an_echoed_prompt() {
        assert_eq!(clean("Write the text now and return only the text.", Some("Fix this")), "");
        assert_eq!(clean("Fix this", Some("Fix this")), "");
        assert_eq!(clean("Request: say hi\nHi there!", None), "Hi there!");
    }
}
