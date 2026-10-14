//! Ordinary English words, to tell them apart from names and terms the speech model may have
//! misheard (the Mac app asks the system spell checker; Windows and Linux don't have a
//! dependable one, so Kudu carries a compact list, ~0.75 MB compressed).

use std::io::Read;
use std::sync::LazyLock;

static WORDS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    let mut text = String::new();
    let compressed: &[u8] = include_bytes!("../../assets/words.txt.bz2");
    if let Err(error) = bzip2::read::BzDecoder::new(compressed).read_to_string(&mut text) {
        log::error!("Couldn't read the word list: {error}");
        return Vec::new();
    }
    let text: &'static str = Box::leak(text.into_boxed_str());
    let mut words: Vec<&'static str> = text.lines().collect();
    words.sort_unstable();
    words
});

/// Loads the list ahead of the first dictation.
pub fn preload() {
    LazyLock::force(&WORDS);
}

fn contains(word: &str) -> bool {
    WORDS.binary_search(&word).is_ok()
}

/// Whether `word` is an ordinary English word, including common inflections ("emails",
/// "running", "studied", "quickly").
pub fn is_known_word(word: &str) -> bool {
    let word = word.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
    let word = word.trim_end_matches("'s").trim_end_matches("’s");
    if word.is_empty() || contains(word) {
        return true;
    }
    let stem = |suffix: &str| word.strip_suffix(suffix).filter(|s| s.len() >= 2);
    let candidates = [
        stem("s").map(str::to_string),
        stem("es").map(str::to_string),
        stem("ies").map(|s| format!("{s}y")),
        stem("ied").map(|s| format!("{s}y")),
        stem("ed").map(str::to_string),
        stem("d").map(str::to_string),
        stem("ing").map(str::to_string),
        stem("ing").map(|s| format!("{s}e")),
        stem("ly").map(str::to_string),
        stem("er").map(str::to_string),
        stem("est").map(str::to_string),
        // Doubled consonants: running → run, stopped → stop.
        stem("ing").or_else(|| stem("ed")).filter(|s| doubled(s)).map(|s| s[..s.len() - 1].to_string()),
    ];
    candidates.into_iter().flatten().any(|c| contains(&c))
}

fn doubled(stem: &str) -> bool {
    let bytes = stem.as_bytes();
    bytes.len() >= 3 && bytes[bytes.len() - 1] == bytes[bytes.len() - 2]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_everyday_words_and_their_forms() {
        for word in ["the", "heat", "map", "emails", "running", "studied", "quickly", "Price", "pride's", "website"] {
            assert!(is_known_word(word), "{word}");
        }
    }

    #[test]
    fn does_not_know_misheard_terms() {
        for word in ["hitmap", "sidarth", "kudoo", "supabaze"] {
            assert!(!is_known_word(word), "{word}");
        }
    }
}
