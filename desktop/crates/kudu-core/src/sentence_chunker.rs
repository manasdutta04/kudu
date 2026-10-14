//! Splits a transcript into sentence-aligned chunks for formatting while the user speaks.
//! Chunks are built greedily from the start, so the same prefix always gives the same chunks
//! (a chunk formatted earlier can be reused if the transcript up to it hasn't changed).

use crate::text::trim_spaces;
use crate::usage_stats::word_count;

/// Default size of a stable chunk, in words.
pub const DEFAULT_MINIMUM_WORDS: usize = 12;

/// Stable chunks ready to format, and the rest that may still change.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Chunks {
    pub stable: Vec<String>,
    pub rest: String,
}

/// Sentences, each keeping its punctuation: "Hi. How are you?" → ["Hi.", "How are you?"].
pub fn sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((index, c)) = chars.next() {
        let ends = matches!(c, '.' | '?' | '!');
        // "3.30" or "e.g." in the middle of a word is not a sentence end.
        if ends && chars.peek().is_none_or(|&(_, next)| next.is_whitespace()) {
            let end = index + c.len_utf8();
            let sentence = trim_spaces(&text[start..end]);
            if !sentence.is_empty() {
                sentences.push(sentence.to_owned());
            }
            start = end;
        }
    }
    let rest = trim_spaces(&text[start..]);
    if !rest.is_empty() {
        sentences.push(rest.to_owned());
    }
    sentences
}

/// Groups sentences into chunks of at least `minimum_words` words. The final sentence is never
/// part of a stable chunk while the user may still be speaking it; it is returned in `rest`
/// together with any sentences that don't fill a chunk yet.
pub fn chunks(text: &str, minimum_words: usize) -> Chunks {
    let mut sentences = sentences(text);
    let Some(last) = sentences.pop() else {
        return Chunks::default();
    };
    let mut stable = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut words = 0;
    for sentence in sentences {
        words += word_count(&sentence);
        current.push(sentence);
        if words >= minimum_words {
            stable.push(current.join(" "));
            current.clear();
            words = 0;
        }
    }
    current.push(last);
    Chunks { stable, rest: current.join(" ") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_sentences_keeping_punctuation() {
        assert_eq!(sentences("Hi there. How are you? Great!"), ["Hi there.", "How are you?", "Great!"]);
        assert_eq!(sentences("Meet at 3.30 tomorrow. Ok"), ["Meet at 3.30 tomorrow.", "Ok"]);
    }

    #[test]
    fn groups_into_chunks_and_holds_back_the_last_sentence() {
        let text = "The build is green and the fix is in. Please review it today if you can. Thanks a lot.";
        let chunks = chunks(text, 12);
        assert_eq!(chunks.stable, ["The build is green and the fix is in. Please review it today if you can."]);
        assert_eq!(chunks.rest, "Thanks a lot.");
    }

    #[test]
    fn same_prefix_gives_same_chunks() {
        let early = chunks(
            "One two three four five six seven. Eight nine ten eleven twelve thirteen. And more",
            DEFAULT_MINIMUM_WORDS,
        );
        let later = chunks(
            "One two three four five six seven. Eight nine ten eleven twelve thirteen. And more words here. Final",
            DEFAULT_MINIMUM_WORDS,
        );
        assert_eq!(early.stable, later.stable[..early.stable.len()]);
    }

    #[test]
    fn single_sentence_is_all_rest() {
        assert!(chunks("Just one sentence here", DEFAULT_MINIMUM_WORDS).stable.is_empty());
    }
}
