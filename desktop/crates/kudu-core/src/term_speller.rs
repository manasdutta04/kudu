//! Fixes words the speech model misheard when they sound like a term the user cares about:
//! a dictionary word, or a name or term visible on screen.
//!
//! - "hitmap" → "heatmap", "Sidarth" → "Siddharth": a word the spell checker doesn't know,
//!   that sounds like a term.
//! - "super base" → "Supabase", "heat map" → "heatmap": two or three words that together
//!   sound like a dictionary term written as one.
//! - "Pia" → "Priya": a capitalized name that sounds like a name in the dictionary.
//! - "github" → "GitHub": the same word with the term's inner capitals.
//!
//! Real words are never turned into terms ("pride" stays "pride" next to "Priya"), except by
//! joining, which needs a close match to a dictionary term.

use crate::correction_learner::{is_stopword, phonetic_key_of};
use crate::formatting_guard::edit_distance;
use crate::text::{char_count, letters, lowercase, trim_spaces};
use regex::Regex;
use std::ops::Range;
use std::sync::LazyLock;

static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\p{L}[\p{L}'’]*").unwrap());

/// A term with what matching needs, computed once.
#[derive(Debug)]
struct Term {
    text: String,
    lowercased: String,
    letters: Vec<char>,
    phonetic_key: String,
    length: usize,
}

impl Term {
    fn new(text: String) -> Self {
        let letters = letters(&text);
        Self {
            lowercased: lowercase(&text),
            phonetic_key: phonetic_key_of(&letters),
            length: char_count(&text),
            letters,
            text,
        }
    }

    fn starts_uppercase(&self) -> bool {
        self.text.chars().next().is_some_and(char::is_uppercase)
    }
}

struct Word<'a> {
    range: Range<usize>,
    text: &'a str,
    letters: Vec<char>,
}

/// Spells misheard words as the user's terms. `F` says whether a word is ordinary vocabulary
/// (the system spell checker); core has no spell checker of its own.
pub struct TermSpeller<F> {
    /// Terms the user added or Kudu learned: trusted, so matched more readily.
    dictionary: Vec<Term>,
    /// Names and terms read from the screen.
    screen: Vec<Term>,
    is_known_word: F,
}

impl<F: Fn(&str) -> bool> TermSpeller<F> {
    pub fn new<D, S>(dictionary: D, screen: S, is_known_word: F) -> Self
    where
        D: IntoIterator,
        D::Item: Into<String>,
        S: IntoIterator,
        S::Item: Into<String>,
    {
        Self {
            dictionary: dictionary
                .into_iter()
                .map(Into::into)
                .filter(|term: &String| !term.is_empty())
                .map(Term::new)
                .collect(),
            screen: screen
                .into_iter()
                .map(Into::into)
                .filter(|term: &String| !term.is_empty() && !term.contains('.') && !term.contains('_'))
                .map(Term::new)
                .collect(),
            is_known_word,
        }
    }

    /// The corrected text and how many corrections were made.
    pub fn apply(&self, text: &str) -> (String, usize) {
        if self.dictionary.is_empty() && self.screen.is_empty() {
            return (text.to_owned(), 0);
        }
        let words: Vec<Word> = WORD
            .find_iter(text)
            .map(|found| Word { range: found.range(), text: found.as_str(), letters: letters(found.as_str()) })
            .collect();
        let mut replacements: Vec<(Range<usize>, &str)> = Vec::new();
        let mut index = 0;
        while index < words.len() {
            if let Some((count, term)) = self.joined(&words, index, text) {
                replacements.push((words[index].range.start..words[index + count - 1].range.end, term));
                index += count;
                continue;
            }
            let word = &words[index];
            let at_sentence_start = is_sentence_start(word.range.start, text);
            if let Some(term) = self.single(word, at_sentence_start) {
                if term != word.text {
                    replacements.push((word.range.clone(), term));
                }
            }
            index += 1;
        }
        if replacements.is_empty() {
            return (text.to_owned(), 0);
        }
        let mut result = text.to_owned();
        for (range, term) in replacements.iter().rev() {
            result.replace_range(range.clone(), term);
        }
        (result, replacements.len())
    }

    // MARK: - Matching

    /// Two or three neighbouring words that together are a dictionary term ("heat map").
    fn joined(&self, words: &[Word], index: usize, text: &str) -> Option<(usize, &str)> {
        for count in [3, 2] {
            if index + count > words.len() {
                continue;
            }
            let run = &words[index..index + count];
            // Only words separated by spaces or hyphens, within one phrase.
            let gaps_are_spaces = run
                .windows(2)
                .all(|pair| text[pair[0].range.end..pair[1].range.start].chars().all(|c| c == ' ' || c == '-'));
            if !gaps_are_spaces {
                continue;
            }
            // "the", "a", "to" are never part of a term that was split apart.
            if run.iter().any(|word| is_stopword(&lowercase(word.text)) && char_count(word.text) > 1) {
                continue;
            }
            let letters: Vec<char> = run.iter().flat_map(|word| word.letters.iter().copied()).collect();
            // Spelled-out letters ("Q V") can only be matched by sound; words need a close spelling.
            let spelled_out = run.iter().all(|word| char_count(word.text) == 1);
            let key = spelled_out.then(|| phonetic_key_of(&letters));
            for term in self.dictionary.iter().filter(|term| !term.text.contains(' ') && term.length >= 4) {
                if term.letters.len().abs_diff(letters.len()) > 3 {
                    continue;
                }
                if letters == term.letters
                    || similarity(&letters, &term.letters) >= 0.75
                    || key.as_deref().is_some_and(|key| same_sound(key, term))
                {
                    return Some((count, &term.text));
                }
            }
        }
        None
    }

    fn single(&self, word: &Word, at_sentence_start: bool) -> Option<&str> {
        let length = char_count(word.text);
        if length < 2 {
            return None;
        }
        let lowercased = lowercase(word.text);
        // Already a term, maybe in the wrong case.
        if let Some(exact) = self.dictionary.iter().chain(&self.screen).find(|term| term.lowercased == lowercased) {
            if exact.text == word.text {
                return None;
            }
            let inner_capitals = exact.text.chars().skip(1).any(char::is_uppercase);
            let trusted_name = exact.starts_uppercase()
                && !at_sentence_start
                && self.dictionary.iter().any(|term| term.text == exact.text);
            return (inner_capitals || trusted_name).then_some(exact.text.as_str());
        }
        if is_stopword(&lowercased) {
            return None;
        }

        let key = phonetic_key_of(&word.letters);

        if !(self.is_known_word)(word.text) {
            return closest(self.dictionary.iter(), word, &key, length, 0.5)
                .or_else(|| closest(self.screen.iter(), word, &key, length, 0.5));
        }
        // A capitalized word mid-sentence is a name: the speech model heard some name. If it
        // sounds like a name in the dictionary, it was most likely that one ("Pia" → "Priya").
        if word.text.chars().next().is_some_and(char::is_uppercase) && !at_sentence_start && length >= 3 {
            let names = self.dictionary.iter().filter(|term| term.starts_uppercase());
            return closest(names, word, &key, length, 0.6);
        }
        None
    }
}

impl<F> std::fmt::Debug for TermSpeller<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TermSpeller")
            .field("dictionary", &self.dictionary.iter().map(|term| &term.text).collect::<Vec<_>>())
            .field("screen", &self.screen.iter().map(|term| &term.text).collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

// MARK: - Helpers

/// The term most like `word` (the first of equally close ones), if it's close enough in
/// spelling, or sounds the same and is somewhat close.
fn closest<'t>(
    terms: impl Iterator<Item = &'t Term>,
    word: &Word,
    key: &str,
    length: usize,
    minimum: f64,
) -> Option<&'t str> {
    let mut best: Option<(&Term, f64)> = None;
    for term in terms {
        if term.text.contains(' ') || term.length.abs_diff(length) > 3 {
            continue;
        }
        let score = similarity(&word.letters, &term.letters);
        let sounds_alike = same_sound(key, term) && score >= 0.4;
        if (score >= minimum || sounds_alike) && best.is_none_or(|(_, best)| score > best) {
            best = Some((term, score));
        }
    }
    best.map(|(term, _)| term.text.as_str())
}

fn similarity(a: &[char], b: &[char]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    1.0 - edit_distance(a, b) as f64 / a.len().max(b.len()) as f64
}

fn same_sound(key: &str, term: &Term) -> bool {
    key.len() >= 2 && key == term.phonetic_key
}

fn is_sentence_start(position: usize, text: &str) -> bool {
    match trim_spaces(&text[..position]).chars().next_back() {
        None => true,
        Some(last) => ".?!:\n•-".contains(last),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// A tiny stand-in for the system spell checker.
    static VOCABULARY: LazyLock<HashSet<&str>> = LazyLock::new(|| {
        [
            "the", "is", "looking", "good", "can", "you", "tag", "heat", "map", "pride", "price", "of", "team", "we",
            "use", "super", "base", "for", "data", "pia", "said", "hi", "could", "do", "that", "paid", "a", "with",
        ]
        .into_iter()
        .collect()
    });

    fn speller(dictionary: &[&str], screen: &[&str]) -> TermSpeller<impl Fn(&str) -> bool> {
        TermSpeller::new(dictionary.iter().copied(), screen.iter().copied(), |word: &str| {
            VOCABULARY.contains(lowercase(word).as_str())
        })
    }

    #[test]
    fn fixes_a_misheard_dictionary_word() {
        let (text, fixes) = speller(&["heatmap"], &[]).apply("The hitmap is looking good.");
        assert_eq!(text, "The heatmap is looking good.");
        assert_eq!(fixes, 1);
    }

    #[test]
    fn joins_words_spoken_apart_into_a_dictionary_term() {
        assert_eq!(speller(&["heatmap"], &[]).apply("The heat map is looking good.").0, "The heatmap is looking good.");
        assert_eq!(speller(&["Supabase"], &[]).apply("We use super base for data.").0, "We use Supabase for data.");
    }

    #[test]
    fn turns_a_sound_alike_name_into_the_dictionary_name() {
        assert_eq!(speller(&["Priya"], &[]).apply("Can you tag Pia?").0, "Can you tag Priya?");
        // At a sentence start a capital says nothing: leave it.
        assert_eq!(speller(&["Priya"], &[]).apply("Pia said hi.").0, "Pia said hi.");
    }

    #[test]
    fn never_turns_real_words_into_terms() {
        let text = "The pride of the team is the price we paid.";
        assert_eq!(speller(&["Priya", "heatmap"], &["Priya"]).apply(text).0, text);
        assert_eq!(speller(&["Kudu"], &[]).apply("We could do that.").0, "We could do that.");
    }

    #[test]
    fn uses_screen_terms_for_unknown_words_and_inner_capitals() {
        let speller = speller(&[], &["Siddharth", "GitHub"]);
        assert_eq!(speller.apply("Thanks Sidarth, push it to github.").0, "Thanks Siddharth, push it to GitHub.");
    }

    #[test]
    fn spelled_out_letters_match_by_sound() {
        assert_eq!(speller(&["Kudu"], &[]).apply("try Q D now").0, "try Kudu now");
    }
}
