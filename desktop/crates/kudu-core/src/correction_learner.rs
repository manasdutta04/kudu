//! Learns from the corrections people make right after a dictation. It word-aligns what Kudu
//! typed with what the field says now and keeps only edits that look like a fixed spelling of
//! a name or term, never rewording.
//!
//! - "I pushed it to git hub" → "I pushed it to GitHub": learns git hub → GitHub.
//! - "Thanks kudoo team" → "Thanks Kudu team": learns kudoo → Kudu.
//! - "the" → "a", "send it" → "email it", or a rewritten sentence: learns nothing.

use crate::formatting_guard::edit_distance;
use crate::text::{is_punctuation, is_symbol, letters, lowercase};
use std::collections::HashSet;
use std::sync::LazyLock;

/// A spelling Kudu should remember: it typed `heard`, the user corrected it to `written`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LearnedCorrection {
    pub heard: String,
    pub written: String,
}

impl LearnedCorrection {
    pub fn new(heard: impl Into<String>, written: impl Into<String>) -> Self {
        Self { heard: heard.into(), written: written.into() }
    }
}

/// More than this share of words changed means the user rewrote the text, not fixed it.
pub const MAXIMUM_CHANGED_SHARE: f64 = 0.5;
pub const MAXIMUM_CANDIDATES: usize = 3;
pub const MAXIMUM_PHRASE_WORDS: usize = 3;

/// The corrections worth remembering between what was `inserted` and the `edited` text.
///
/// `known` are spellings already in the dictionary (compared case-insensitively), which
/// aren't learned again. `is_known_word` says whether a word is ordinary vocabulary (the
/// system spell checker). Swapping one ordinary word for another ("hit" → "heat") is only
/// safe with its neighbour, so it's learned as a phrase ("hit map" → "heat map"); learning
/// the bare word would change every "hit" from then on.
pub fn learn(
    inserted: &str,
    edited: &str,
    known: &[String],
    is_known_word: impl Fn(&str) -> bool,
) -> Vec<LearnedCorrection> {
    let before = tokens(inserted);
    let after = tokens(edited);
    if before.is_empty() || after.is_empty() || before == after {
        return Vec::new();
    }

    let operations = align(&before, &after);
    let changed = operations.iter().filter(|operation| operation.kind != Kind::Match).count();
    if changed as f64 / before.len() as f64 > MAXIMUM_CHANGED_SHARE {
        return Vec::new();
    }

    let known_lowercased: HashSet<String> = known.iter().map(|word| lowercase(word)).collect();
    let mut results: Vec<LearnedCorrection> = Vec::new();
    let mut index = 0;
    while index < operations.len() {
        if operations[index].kind == Kind::Match {
            index += 1;
            continue;
        }
        // A run of edits between two matching words (or the text's edges).
        let mut end = index;
        while end < operations.len() && operations[end].kind != Kind::Match {
            end += 1;
        }
        let run = &operations[index..end];
        let from: Vec<&str> = run.iter().filter_map(|operation| operation.before).collect();
        let to: Vec<&str> = run.iter().filter_map(|operation| operation.after).collect();
        let at_sentence_start = index == 0
            || operations[index - 1]
                .before
                .is_some_and(|word| ".?!:".contains(word.chars().next_back().unwrap_or(' ')));
        if let Some(mut correction) = candidate(&from, &to, at_sentence_start) {
            let ordinary_swap = from.len() == 1
                && to.len() == 1
                && correction.heard != lowercase(&correction.written)
                && is_known_word(from[0])
                && is_known_word(to[0]);
            if ordinary_swap {
                // Anchor it to the unchanged word after it (or before it, at the end).
                let next = operations.get(end).and_then(|operation| operation.after);
                let previous = if index > 0 { operations[index - 1].after } else { None };
                if let Some(next) = next {
                    correction = LearnedCorrection {
                        heard: format!("{} {}", correction.heard, lowercase(next)),
                        written: format!("{} {next}", correction.written),
                    };
                } else if let Some(previous) = previous {
                    correction = LearnedCorrection {
                        heard: format!("{} {}", lowercase(previous), correction.heard),
                        written: format!("{previous} {}", correction.written),
                    };
                } else {
                    index = end;
                    continue;
                }
            }
            if !known_lowercased.contains(&lowercase(&correction.written))
                && !results.iter().any(|result| result.written == correction.written)
            {
                results.push(correction);
            }
        }
        index = end;
    }
    results.truncate(MAXIMUM_CANDIDATES);
    results
}

// MARK: - Deciding

fn candidate(from: &[&str], to: &[&str], at_sentence_start: bool) -> Option<LearnedCorrection> {
    let phrase = 1..=MAXIMUM_PHRASE_WORDS;
    if !phrase.contains(&from.len()) || !phrase.contains(&to.len()) {
        return None;
    }
    let heard = from.iter().map(|word| bare(word)).collect::<Vec<_>>().join(" ");
    let written = to.iter().map(|word| bare(word)).collect::<Vec<_>>().join(" ");
    if heard.is_empty() || written.is_empty() || heard == written {
        return None;
    }

    // Numbers, contractions and everyday words aren't vocabulary.
    if heard.chars().chain(written.chars()).any(char::is_numeric) {
        return None;
    }
    if written.contains('\'') || written.contains('’') {
        return None;
    }
    let heard_lowercased = lowercase(&heard);
    let written_lowercased = lowercase(&written);
    if to.len() == 1 && is_stopword(&written_lowercased) {
        return None;
    }
    if from.len() == 1 && to.len() == 1 && is_stopword(&heard_lowercased) {
        return None;
    }

    if heard_lowercased == written_lowercased {
        // A capitalization fix. At a sentence start it says nothing about the word itself,
        // unless the new form has a capital inside ("GitHub", "iPhone").
        let inner_capital = written.chars().skip(1).any(char::is_uppercase);
        if !written.chars().any(char::is_uppercase) || (at_sentence_start && !inner_capital) {
            return None;
        }
        return Some(LearnedCorrection { heard: heard_lowercased, written });
    }
    if !sounds_alike(&heard, &written) {
        return None;
    }
    Some(LearnedCorrection { heard: heard_lowercased, written })
}

/// Whether `heard` is plausibly a mis-hearing or misspelling of `written`: close in spelling,
/// or the same consonant sounds ("coo doo" / "Kudu", "priyah" / "Priya").
pub fn sounds_alike(heard: &str, written: &str) -> bool {
    let a = letters(heard);
    let b = letters(written);
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let similarity = 1.0 - edit_distance(&a, &b) as f64 / a.len().max(b.len()) as f64;
    if similarity >= 0.45 {
        return true;
    }
    let key = phonetic_key_of(&a);
    key.len() >= 2 && key == phonetic_key_of(&b)
}

/// Soundex-style consonant skeleton of lowercase letters, including the first letter:
/// "coodoo" → "23", "kudu" → "23".
pub fn phonetic_key(letters: &str) -> String {
    phonetic_key_of(&letters.chars().collect::<Vec<_>>())
}

pub(crate) fn phonetic_key_of(letters: &[char]) -> String {
    let mut key = String::new();
    let mut last: Option<char> = None;
    for &c in letters {
        let code = match c {
            'b' | 'f' | 'p' | 'v' => Some('1'),
            'c' | 'g' | 'j' | 'k' | 'q' | 's' | 'x' | 'z' => Some('2'),
            'd' | 't' => Some('3'),
            'l' => Some('4'),
            'm' | 'n' => Some('5'),
            'r' => Some('6'),
            _ => None,
        };
        if let Some(code) = code {
            if Some(code) != last {
                key.push(code);
            }
        }
        last = code;
    }
    key
}

// MARK: - Alignment

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Match,
    Substitute,
    Insert,
    Delete,
}

#[derive(Debug)]
struct Operation<'a> {
    kind: Kind,
    before: Option<&'a str>,
    after: Option<&'a str>,
}

/// Words, keeping their case and inner punctuation ("GitHub", "don't"), without the
/// punctuation around them.
pub(crate) fn tokens(text: &str) -> Vec<&str> {
    text.split_whitespace().map(bare).filter(|word| !word.is_empty()).collect()
}

fn bare(word: &str) -> &str {
    word.trim_matches(|c: char| is_punctuation(c) || is_symbol(c))
}

/// Word-level edit alignment with backtrace. Words compare exactly (case-sensitive), so a
/// capitalization fix shows up as a substitution.
fn align<'a>(a: &[&'a str], b: &[&'a str]) -> Vec<Operation<'a>> {
    let width = b.len() + 1;
    let mut cost = vec![0usize; (a.len() + 1) * width];
    for i in 0..=a.len() {
        cost[i * width] = i;
    }
    for (j, first_row) in cost[..width].iter_mut().enumerate() {
        *first_row = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let same = a[i - 1] == b[j - 1];
            cost[i * width + j] = (cost[(i - 1) * width + j] + 1)
                .min(cost[i * width + j - 1] + 1)
                .min(cost[(i - 1) * width + j - 1] + usize::from(!same));
        }
    }
    let mut operations = Vec::with_capacity(a.len().max(b.len()));
    let (mut i, mut j) = (a.len(), b.len());
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && cost[i * width + j] == cost[(i - 1) * width + j - 1] + usize::from(a[i - 1] != b[j - 1]) {
            let kind = if a[i - 1] == b[j - 1] { Kind::Match } else { Kind::Substitute };
            operations.push(Operation { kind, before: Some(a[i - 1]), after: Some(b[j - 1]) });
            i -= 1;
            j -= 1;
        } else if i > 0 && cost[i * width + j] == cost[(i - 1) * width + j] + 1 {
            operations.push(Operation { kind: Kind::Delete, before: Some(a[i - 1]), after: None });
            i -= 1;
        } else {
            operations.push(Operation { kind: Kind::Insert, before: None, after: Some(b[j - 1]) });
            j -= 1;
        }
    }
    operations.reverse();
    operations
}

static STOPWORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "a", "an", "the", "and", "or", "but", "if", "of", "to", "in", "on", "at", "by", "for", "with", "from", "is",
        "are", "was", "were", "be", "been", "am", "do", "does", "did", "have", "has", "had", "it", "its", "this",
        "that", "these", "those", "i", "you", "he", "she", "we", "they", "me", "him", "her", "us", "them", "my",
        "your", "his", "our", "their", "so", "not", "no", "yes", "as", "up", "out", "can", "will", "just", "then",
        "than", "there", "here", "what", "when", "where", "who", "how", "why", "all", "any", "some", "about", "into",
        "over", "also", "very", "too", "ok", "okay",
    ]
    .into_iter()
    .collect()
});

/// Everyday words that are never vocabulary on their own (lowercase input).
pub(crate) fn is_stopword(lowercased: &str) -> bool {
    STOPWORDS.contains(lowercased)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn learn_plain(inserted: &str, edited: &str) -> Vec<LearnedCorrection> {
        learn(inserted, edited, &[], |_| false)
    }

    #[test]
    fn learns_a_misspelled_name() {
        let learned = learn_plain("Thanks kudoo team, see you soon.", "Thanks Kudu team, see you soon.");
        assert_eq!(learned, [LearnedCorrection::new("kudoo", "Kudu")]);
    }

    #[test]
    fn learns_a_multi_word_mishearing() {
        let learned = learn_plain("I pushed it to git hub today.", "I pushed it to GitHub today.");
        assert_eq!(learned, [LearnedCorrection::new("git hub", "GitHub")]);
    }

    #[test]
    fn learns_sound_alikes_that_are_spelled_differently() {
        let learned = learn_plain("We use coo doo for dictation.", "We use Kudu for dictation.");
        assert_eq!(learned, [LearnedCorrection::new("coo doo", "Kudu")]);
    }

    #[test]
    fn learns_proper_noun_capitalization_mid_sentence() {
        let learned = learn_plain("Please ask priya about it.", "Please ask Priya about it.");
        assert_eq!(learned, [LearnedCorrection::new("priya", "Priya")]);
    }

    #[test]
    fn ignores_capitalization_at_sentence_start() {
        assert!(learn_plain("sounds good to me.", "Sounds good to me.").is_empty());
    }

    #[test]
    fn ignores_rewording_and_everyday_words() {
        assert!(learn_plain("Send it to the team.", "Send it to a team.").is_empty());
        assert!(learn_plain("Please send it today.", "Please email it today.").is_empty());
    }

    #[test]
    fn ignores_rewrites() {
        assert!(learn_plain("Can we meet tomorrow at noon?", "Lunch on Friday works better for me.").is_empty());
    }

    #[test]
    fn ignores_numbers_and_known_words() {
        assert!(learn_plain("It costs 40 dollars.", "It costs 45 dollars.").is_empty());
        assert!(learn("Ask kudoo now.", "Ask Kudu now.", &["kudu".to_owned()], |_| false).is_empty());
    }

    #[test]
    fn phonetic_keys_match_sound_alikes() {
        assert_eq!(phonetic_key("coodoo"), phonetic_key("kudu"));
        assert!(sounds_alike("priyah", "Priya"));
        assert!(!sounds_alike("send", "email"));
    }

    #[test]
    fn learns_a_misheard_word_fixed_in_place() {
        let learned = learn_plain(
            "And inside the page the hitmap is looking good",
            "And inside the page the heatmap is looking good",
        );
        assert_eq!(learned, [LearnedCorrection::new("hitmap", "heatmap")]);
    }

    #[test]
    fn learns_a_swap_of_ordinary_words_only_as_a_phrase() {
        let vocabulary: HashSet<&str> = ["the", "hit", "heat", "map", "is", "ready"].into_iter().collect();
        let learned = learn("The hit map is ready", "The heat map is ready", &[], |word| {
            vocabulary.contains(lowercase(word).as_str())
        });
        assert_eq!(learned, [LearnedCorrection::new("hit map", "heat map")]);
    }

    #[test]
    fn learns_a_misheard_name() {
        let learned = learn_plain("Can you tag Pia on this?", "Can you tag Priya on this?");
        assert_eq!(learned, [LearnedCorrection::new("pia", "Priya")]);
    }
}
