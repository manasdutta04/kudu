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
/// A thousands separator between digits: "1,250" is one number.
static DIGIT_GROUP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d),(\d{3})\b").unwrap());

/// Spoken enumerators a list drops for bullets or one item per line.
const ENUMERATORS: &[&str] = &[
    "first", "firstly", "second", "secondly", "third", "thirdly", "fourth", "fifth", "finally", "next", "number",
    "things", "items", "points", "and", "also", "then", "add", "step", "steps", "bullet", "bullets",
];

/// Whether a (canonical) word is a spoken enumerator. Words are compared after numbers are
/// written as digits, so "number one" and "step two" arrive here as "1" and "2".
fn is_enumerator(word: &str) -> bool {
    ENUMERATORS.contains(&word) || matches!(word, "1" | "2" | "3" | "4" | "5")
}

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
        before.retain(|word| !is_enumerator(word));
        after.retain(|word| !is_enumerator(word));
    }
    // A single to-do item drops its spoken "add": "add buy a charger" → "Buy a charger".
    if before.first().is_some_and(|word| word == "add") && after.first().is_none_or(|word| word != "add") {
        before.remove(0);
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
    // "π" is notation for the spoken "pi".
    let notation = formatted.replace('π', " pi ");
    let unexpected = letter_words(&notation).iter().any(|word| {
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
    // A stray backtick, or a quote with no partner, is a glitch rather than code (unless the
    // speaker dictated a quote mark).
    let unpaired = |c: char| output.matches(c).count() % 2 == 1;
    if unpaired('`') || (unpaired('"') && !lowercase(original).contains("quote")) {
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
    let mut lower = lowercase(text);
    // "1,250,000" is one number: join thousands separators (repeatedly, as matches overlap).
    loop {
        let joined = DIGIT_GROUP.replace_all(&lower, "${1}${2}").into_owned();
        if joined == lower {
            break;
        }
        lower = joined;
    }
    let mut parts = Vec::new();
    for token in lower.split(|c: char| !(c.is_alphabetic() || c.is_numeric() || c == '\'' || c == '’')) {
        split_digits_and_letters(token, |part| {
            parts.push(if part.contains('’') { part.replace('’', "'") } else { part.to_owned() });
        });
    }
    collapse_spoken_numbers(parts).into_iter().filter_map(canonical).collect()
}

#[derive(Clone, Copy, PartialEq)]
enum NumberWord {
    /// zero to nine
    Unit,
    /// ten to nineteen
    Teen,
    /// twenty, thirty … ninety
    Tens,
    Hundred,
    /// thousand, million
    Scale,
}

fn number_word(word: &str) -> Option<(u64, NumberWord)> {
    use NumberWord::*;
    const UNITS: [&str; 10] = ["zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine"];
    const TEENS: [&str; 10] =
        ["ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen"];
    const TENS: [&str; 8] = ["twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];
    if let Some(i) = UNITS.iter().position(|w| *w == word) {
        return Some((i as u64, Unit));
    }
    if let Some(i) = TEENS.iter().position(|w| *w == word) {
        return Some((10 + i as u64, Teen));
    }
    if let Some(i) = TENS.iter().position(|w| *w == word) {
        return Some((20 + 10 * i as u64, Tens));
    }
    match word {
        "hundred" => Some((100, Hundred)),
        "thousand" => Some((1_000, Scale)),
        "million" => Some((1_000_000, Scale)),
        _ => None,
    }
}

/// Writes spoken numbers as digits, so they compare equal to what the model writes: "one
/// thousand two hundred and fifty" → "1250", "a hundred and twenty eight" → "128", "forty
/// five" → "45". Numbers said one after another stay separate: "six thirty" → "6", "30" (a
/// time), "twenty twenty six" → "20", "26".
fn collapse_spoken_numbers(parts: Vec<String>) -> Vec<String> {
    use NumberWord::*;
    let is_number = |word: Option<&String>| word.and_then(|w| number_word(w)).is_some();
    let mut out = Vec::with_capacity(parts.len());
    let mut i = 0;
    while i < parts.len() {
        let starts_with_a = parts[i] == "a"
            && parts.get(i + 1).and_then(|w| number_word(w)).is_some_and(|(_, kind)| matches!(kind, Hundred | Scale));
        if number_word(&parts[i]).is_none() && !starts_with_a {
            out.push(parts[i].clone());
            i += 1;
            continue;
        }
        let (mut total, mut current) = (0u64, 0u64);
        let mut last: Option<NumberWord> = None;
        if starts_with_a {
            current = 1;
            last = Some(Unit);
            i += 1;
        }
        while i < parts.len() {
            // "and" joins "two hundred and fifty", "a thousand and one".
            if parts[i] == "and" && matches!(last, Some(Hundred | Scale)) && is_number(parts.get(i + 1)) {
                i += 1;
                continue;
            }
            let Some((value, kind)) = number_word(&parts[i]) else { break };
            let fits = matches!(
                (last, kind),
                (None, _)
                    | (Some(Hundred | Scale), Unit | Teen | Tens)
                    | (Some(Tens), Unit)
                    | (Some(Unit | Teen | Tens), Hundred)
                    | (Some(Unit | Teen | Tens | Hundred), Scale)
            );
            if !fits {
                break;
            }
            match kind {
                Unit | Teen | Tens => current += value,
                Hundred => current = current.max(1) * 100,
                Scale => {
                    total += current.max(1) * value;
                    current = 0;
                }
            }
            last = Some(kind);
            i += 1;
        }
        out.push((total + current).to_string());
    }
    out
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
    fn spoken_numbered_and_bulleted_lists_are_accepted() {
        let numbered = "number one unplug it number two wait ten seconds number three plug it back in";
        assert!(accept(numbered, "1. Unplug it\n2. Wait 10 seconds\n3. Plug it back in", Light).is_some());
        let steps = "step one open settings step two choose models step three press download";
        assert!(accept(steps, "1. Open settings\n2. Choose models\n3. Press download", Light).is_some());
        let bullets = "bullet milk bullet eggs bullet bread";
        assert!(accept(bullets, "• Milk\n• Eggs\n• Bread", Light).is_some());
    }

    #[test]
    fn multi_part_spoken_numbers_match_their_digits() {
        assert!(accept("the total is one thousand two hundred and fifty dollars", "The total is $1,250.", Light).is_some());
        assert!(accept("we invited a hundred and twenty eight people", "We invited 128 people.", Light).is_some());
        assert!(accept("the budget is twelve thousand dollars", "The budget is $12,000.", Light).is_some());
        assert!(accept("forty five people came at six thirty", "45 people came at 6:30.", Light).is_some());
        assert_eq!(words("one thousand two hundred and fifty"), ["1250"]);
        assert_eq!(words("$1,250,000"), ["1250000"]);
        assert_eq!(words("six thirty and twenty twenty six"), ["6", "30", "and", "20", "26"]);
        assert_eq!(words("a hundred and one"), ["101"]);
    }

    #[test]
    fn pi_is_spoken_math() {
        assert!(accept("the area is pi r squared", "The area is πr²", Light).is_some());
    }

    #[test]
    fn a_single_to_do_item_drops_its_add() {
        assert!(accept("add buy a charger", "Buy a charger", Light).is_some());
        assert!(accept("add the numbers together", "Add the numbers together.", Light).is_some());
    }

    #[test]
    fn code_guard_rejects_a_stray_backtick_or_quote() {
        let spoken = "git commit dash m fixed the login bug";
        assert!(accept_code(spoken, "git commit -m \"fixed the login bug\"").is_some());
        assert_eq!(accept_code(spoken, "git commit -m \"fixed the login bug\"`"), None);
        assert_eq!(accept_code(spoken, "git commit -m \"fixed the login bug"), None);
        assert!(accept_code("echo quote hello", "echo \"hello").is_some());
    }

    #[test]
    fn words_canonicalize_numbers_and_units() {
        assert_eq!(words("Two kilos at 8pm, 50 percent’s"), ["2", "kg", "at", "8", "pm", "50", "percent's"]);
        assert_eq!(edit_distance(&["a", "b", "c"], &["a", "c"]), 1);
        assert_eq!(edit_distance::<&str>(&[], &["a"]), 1);
    }
}
