//! Picks the names and code terms out of the text visible on screen, so dictation can spell
//! them the way they're written there: "Siddharth", "GitHub", "userService.ts", "@priya".

use crate::text::{char_count, lowercase};
use std::collections::HashMap;

pub const MAXIMUM_TERMS: usize = 40;

const SURROUNDING_PUNCTUATION: &[char] =
    &['.', ',', ';', ':', '!', '?', '"', '\'', '(', ')', '[', ']', '{', '}', '<', '>', '“', '”', '‘', '’'];

/// The terms in `texts`, most frequent first (ties alphabetically), at most [`MAXIMUM_TERMS`].
pub fn terms<S: AsRef<str>>(texts: &[S]) -> Vec<String> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for text in texts {
        let mut previous_ends_sentence = true;
        for raw in text.as_ref().split_whitespace() {
            let sentence_start = previous_ends_sentence;
            previous_ends_sentence = raw.chars().next_back().is_some_and(|c| ".!?\n".contains(c));
            let word = raw.trim_matches(SURROUNDING_PUNCTUATION);
            let length = char_count(word);
            if !(3..=40).contains(&length) || !word.chars().next().is_some_and(|c| c.is_alphabetic() || c == '@') {
                continue;
            }
            let handle = word.starts_with('@');
            let bare = if handle { &word[1..] } else { word };
            if char_count(bare) < 3
                || bare.contains('/')
                || bare.contains("://")
                || !bare.chars().all(|c| c.is_alphabetic() || c.is_numeric() || "._-".contains(c))
            {
                continue;
            }
            if is_term(bare, handle, sentence_start) {
                *counts.entry(bare).or_default() += 1;
            }
        }
    }
    let mut sorted: Vec<(&str, usize)> = counts.into_iter().collect();
    sorted.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    sorted.into_iter().take(MAXIMUM_TERMS).map(|(term, _)| term.to_owned()).collect()
}

fn is_term(word: &str, handle: bool, sentence_start: bool) -> bool {
    if handle {
        return true;
    }
    if !word.chars().any(char::is_alphabetic) {
        return false;
    }
    // Code: userService.ts, session_token, fetchWithAuth.
    if word.contains('.')
        && word.split('.').filter(|part| !part.is_empty()).count() >= 2
        && word.chars().next_back().is_some_and(char::is_alphabetic)
    {
        return true;
    }
    if word.contains('_') {
        return true;
    }
    if word.chars().skip(1).any(char::is_uppercase) && word.chars().any(char::is_lowercase) {
        return true;
    }
    // Names: capitalized words not at a sentence start, and not common words.
    let mut chars = word.chars();
    let starts_uppercase = chars.next().is_some_and(char::is_uppercase);
    if !starts_uppercase || sentence_start || !chars.all(|c| c.is_lowercase() || c == '-' || c == '\'') {
        return false;
    }
    !COMMON_CAPITALIZED.contains(&lowercase(word).as_str())
}

#[rustfmt::skip]
const COMMON_CAPITALIZED: &[&str] = &[
    "the", "this", "that", "these", "those", "and", "but", "for", "you", "your", "our", "his", "her", "its", "monday",
    "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday", "january", "february", "march", "april", "june",
    "july", "august", "september", "october", "november", "december", "today", "tomorrow", "yesterday", "new", "open",
    "save", "edit", "view", "file", "help", "window", "search", "settings", "home", "inbox", "draft", "drafts", "sent",
    "reply", "send", "cancel", "done", "ok",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_names_handles_and_code() {
        let terms = terms(&[
            "Thanks Siddharth, I pushed the fix to GitHub. Ask @priya to review userService.ts and session_token.",
            "Meeting with Siddharth on Friday.",
        ]);
        assert_eq!(terms.first().map(String::as_str), Some("Siddharth")); // seen twice
        for expected in ["GitHub", "priya", "userService.ts", "session_token"] {
            assert!(terms.iter().any(|term| term == expected), "{expected} in {terms:?}");
        }
    }

    #[test]
    fn skips_sentence_starts_common_words_and_links() {
        let terms = terms(&["Today we shipped. The build is green. See https://example.com/path on Friday."]);
        assert!(!terms.iter().any(|term| term == "Today"));
        assert!(!terms.iter().any(|term| term == "The"));
        assert!(!terms.iter().any(|term| term == "Friday"));
        assert!(!terms.iter().any(|term| term.contains("example")));
    }
}
