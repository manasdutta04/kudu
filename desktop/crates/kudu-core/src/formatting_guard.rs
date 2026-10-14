//! Decides whether text returned by the language model is a faithful formatting of what was
//! dictated, or whether the model rewrote it, answered it, or wrapped it in commentary.
//! Formatting changes punctuation, casing, line breaks and number style — not the words.

use crate::editing_strength::EditingStrength;
use crate::formatting_need::has_math_cues;
use crate::text::{char_count, lowercase, trim_spaces};
use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

static LIST_MARKER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(^|\n)\s*(\d+[.)]|•|-)\s+").unwrap());
static NUMBERED_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(^|\n)\s*\d+[.)]").unwrap());

/// Spoken enumerators a list drops for bullets or one item per line.
const ENUMERATORS: &[&str] = &[
    "first", "firstly", "second", "secondly", "third", "thirdly", "fourth", "fifth", "finally", "next", "number",
    "one", "two", "three", "four", "five", "things", "items", "points", "and", "also", "then", "add",
];

const MATH_NAMES: &[&str] = &["sin", "cos", "tan", "log", "ln", "lim", "d", "dx", "dy", "dt", "e", "pi"];

#[rustfmt::skip]
const SPOKEN_SYMBOLS: &[&str] = &[
    "dot", "dash", "hyphen", "underscore", "slash", "backslash", "equals", "quote", "quotes", "colon", "semicolon",
    "comma", "period", "space", "camel", "case", "snake", "open", "close", "paren", "bracket",
];

/// The cleaned-up model output if it is an acceptable formatting of `original`, otherwise
/// `None`.
pub fn accept(original: &str, formatted: &str, strength: EditingStrength) -> Option<String> {
    let mut output = formatted.trim();
    // Models sometimes wrap the answer in quotes or a code fence.
    for (open, close) in [("\"", "\""), ("“", "”"), ("```", "```")] {
        if output.starts_with(open)
            && output.ends_with(close)
            && char_count(output) > char_count(open) + char_count(close)
        {
            output = output[open.len()..output.len() - close.len()].trim();
        }
    }
    if output.is_empty() {
        return None;
    }

    // Spoken math becomes notation ("x squared" → "x²"): judged by its own rule.
    if has_math_cues(original) {
        return accept_math(original, output);
    }

    let mut before = words(original);
    // List markers ("1.", "•") are layout, not words.
    let mut after = words(&LIST_MARKER.replace_all(output, "${1}"));
    // A list drops its spoken enumerators ("three things first… second…", "add …") for
    // bullets or one item per line.
    let lines = output.split('\n').filter(|line| !trim_spaces(line).is_empty()).count();
    if output.contains('•') || lines >= 3 || NUMBERED_LINE.is_match(output) {
        before.retain(|word| !ENUMERATORS.contains(&word.as_str()));
        after.retain(|word| !ENUMERATORS.contains(&word.as_str()));
    }
    if before.is_empty() {
        return None;
    }

    let length_change = (after.len() as f64 - before.len() as f64).abs() / before.len() as f64;
    if length_change > strength.maximum_length_change() {
        return None;
    }
    let change = edit_distance(&before, &after) as f64 / before.len() as f64;
    if change > strength.maximum_word_change() {
        return None;
    }
    Some(output.to_owned())
}

/// Math notation replaces spoken words with symbols, so it can't be compared word by word.
/// It is accepted when every word left in it was spoken (or is a math name like sin) and it
/// isn't much longer than what was said: an answer or an explanation would fail both.
pub(crate) fn accept_math(original: &str, formatted: &str) -> Option<String> {
    // Letters only: "x²" and "2x" are notation around a spoken "x".
    fn letter_words(text: &str) -> Vec<String> {
        lowercase(text).split(|c: char| !c.is_alphabetic()).filter(|word| !word.is_empty()).map(String::from).collect()
    }
    let spoken: HashSet<String> = letter_words(original).into_iter().collect();
    let unexpected = letter_words(formatted).iter().any(|word| {
        !spoken.contains(word)
            && !MATH_NAMES.contains(&word.as_str())
            && char_count(word) != 1
            && !word.chars().all(char::is_numeric)
            && !spoken.iter().any(|said| said.starts_with(word.as_str()) || word.starts_with(said.as_str()))
    });
    if unexpected || char_count(formatted) > char_count(original) * 3 / 2 + 10 {
        return None;
    }
    Some(formatted.to_owned())
}

/// Code formatting legitimately joins and symbolizes words ("user service dot ts" →
/// "userService.ts"), so code output is compared by its letters instead: once spoken symbol
/// words are dropped, the letters must still be essentially the same.
pub fn accept_code(original: &str, formatted: &str) -> Option<String> {
    let mut output = formatted.trim();
    if output.starts_with("```") && output.ends_with("```") && char_count(output) > 6 {
        output = &output[3..output.len() - 3];
        if let Some(newline) = output.find('\n') {
            if !output[..newline].contains(' ') {
                output = &output[newline + 1..]; // drop a language tag like ```bash
            }
        }
        output = output.trim();
    }
    if let Some(command) = output.strip_prefix("$ ") {
        output = command;
    }
    if output.is_empty() {
        return None;
    }
    fn letters(text: &str, dropping_symbols: bool) -> Vec<char> {
        words(text)
            .iter()
            .filter(|word| !dropping_symbols || !SPOKEN_SYMBOLS.contains(&word.as_str()))
            .flat_map(|word| word.chars())
            .collect()
    }
    let before = letters(original, true);
    let after = letters(output, false);
    if before.is_empty() {
        return None;
    }
    let change = edit_distance(&before, &after) as f64 / before.len() as f64;
    (change <= 0.15).then(|| output.to_owned())
}

/// Lowercased word tokens without punctuation, so formatting-only changes compare equal.
/// Written and spoken forms of numbers and units count as the same word ("two" = "2",
/// "kilo" = "kg"), and "8pm" counts as "8 pm".
pub(crate) fn words(text: &str) -> Vec<String> {
    let lower = lowercase(text);
    let mut words = Vec::new();
    for token in lower.split(|c: char| !(c.is_alphabetic() || c.is_numeric() || c == '\'' || c == '’')) {
        split_digits_and_letters(token, |part| {
            let part = if part.contains('’') { part.replace('’', "'") } else { part.to_owned() };
            if let Some(word) = canonical(part) {
                words.push(word);
            }
        });
    }
    words
}

/// Calls `part` for each run of digits or non-digits: "8pm" → "8", "pm".
fn split_digits_and_letters<'a>(token: &'a str, mut part: impl FnMut(&'a str)) {
    let mut start = 0;
    let mut previous_is_number: Option<bool> = None;
    for (index, c) in token.char_indices() {
        let is_number = c.is_numeric();
        if previous_is_number.is_some_and(|previous| previous != is_number) {
            part(&token[start..index]);
            start = index;
        }
        previous_is_number = Some(is_number);
    }
    if start < token.len() {
        part(&token[start..]);
    }
}

fn canonical(word: String) -> Option<String> {
    // Words that become symbols ($, %, :) when written disappear from the text.
    let mapped = match word.as_str() {
        "percent" | "dollars" | "dollar" | "rupees" | "euros" | "pounds" | "o'clock" => return None,
        "zero" => "0",
        "one" => "1",
        "two" => "2",
        "three" => "3",
        "four" => "4",
        "five" => "5",
        "six" => "6",
        "seven" => "7",
        "eight" => "8",
        "nine" => "9",
        "ten" => "10",
        "eleven" => "11",
        "twelve" => "12",
        "fifteen" => "15",
        "twenty" => "20",
        "thirty" => "30",
        "forty" => "40",
        "fifty" => "50",
        "hundred" => "100",
        "thousand" => "1000",
        "kilo" | "kilos" | "kilogram" | "kilograms" => "kg",
        "gram" | "grams" => "g",
        "litre" | "liter" | "litres" | "liters" => "l",
        "kilometer" | "kilometre" | "kilometers" | "kilometres" => "km",
        "minutes" => "min",
        _ => return Some(word),
    };
    Some(mapped.to_owned())
}

/// Levenshtein distance between two sequences (of words, or of letters).
pub fn edit_distance<T: PartialEq>(a: &[T], b: &[T]) -> usize {
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0; b.len() + 1];
    for (i, item) in a.iter().enumerate() {
        current[0] = i + 1;
        for j in 1..=b.len() {
            let cost = usize::from(*item != b[j - 1]);
            current[j] = (previous[j] + 1).min(current[j - 1] + 1).min(previous[j - 1] + cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use EditingStrength::*;

    #[test]
    fn accepts_punctuation_and_casing() {
        let raw = "hey team quick update the build is green i pushed the fix for the login bug";
        let formatted = "Hey team, quick update — the build is green. I pushed the fix for the login bug.";
        assert_eq!(accept(raw, formatted, Light).as_deref(), Some(formatted));
    }

    #[test]
    fn accepts_lists_and_numbers() {
        let raw = "the plan is first we finish the api second we write tests and third we ship on friday at twenty five past ten";
        let formatted = "The plan is:\n1. We finish the API.\n2. We write tests.\n3. We ship on Friday at 10:25.";
        assert!(accept(raw, formatted, Light).is_some());
    }

    #[test]
    fn strips_wrapping_quotes() {
        assert_eq!(
            accept("ship it today please", "\"Ship it today, please.\"", Light).as_deref(),
            Some("Ship it today, please.")
        );
    }

    #[test]
    fn rejects_an_answer_instead_of_formatting() {
        let raw = "can you write a short and good comment for this post";
        let answer = "Great post! Thanks for sharing these insights — really useful for anyone starting out.";
        assert_eq!(accept(raw, answer, Light), None);
    }

    #[test]
    fn rejects_commentary_and_rewrites() {
        let raw = "let's meet tomorrow at noon to go over the launch plan";
        assert_eq!(
            accept(raw, "Here is the formatted text: Let's meet tomorrow at noon to go over the launch plan.", Light),
            None
        );
        assert_eq!(accept(raw, "Meeting tomorrow, 12 pm: launch plan review.", Light), None);
    }

    #[test]
    fn rejects_empty_output() {
        assert_eq!(accept("hello there", "   ", Light), None);
    }

    #[test]
    fn polished_allows_smoothing_that_light_rejects() {
        let raw = "so um i think we should we should probably move the launch to next week maybe";
        let smoothed = "I think we should probably move the launch to next week.";
        assert_eq!(accept(raw, smoothed, Light), None);
        assert!(accept(raw, smoothed, Polished).is_some());
    }

    #[test]
    fn code_guard_accepts_symbols_and_camel_case() {
        assert!(accept_code(
            "git commit dash m fixed the auth retry loop",
            "git commit -m \"fixed the auth retry loop\""
        )
        .is_some());
        assert!(accept_code(
            "open user service dot ts and rename get user to fetch user",
            "Open userService.ts and rename getUser to fetchUser."
        )
        .is_some());
    }

    #[test]
    fn code_guard_rejects_invented_code() {
        assert_eq!(
            accept_code("rename get user to fetch user", "function fetchUser(id) { return db.users.find(id) }"),
            None
        );
    }

    #[test]
    fn math_is_judged_by_its_own_rule() {
        assert!(accept("x squared plus two x", "x² + 2x", Light).is_some());
        assert_eq!(accept("x squared plus two x", "The answer is that x squared grows faster.", Light), None);
    }

    #[test]
    fn words_canonicalize_numbers_and_units() {
        assert_eq!(words("Two kilos at 8pm, 50 percent’s"), ["2", "kg", "at", "8", "pm", "50", "percent's"]);
        assert_eq!(edit_distance(&["a", "b", "c"], &["a", "c"]), 1);
        assert_eq!(edit_distance::<&str>(&[], &["a"]), 1);
    }
}
