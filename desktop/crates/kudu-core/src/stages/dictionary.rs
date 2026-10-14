use crate::id::new_id;
use crate::pipeline::{TextContext, TextStage};
use crate::text::{char_count, trim_spaces};
use fancy_regex::NoExpand;
use serde::{Deserialize, Serialize};

/// A word or phrase the user wants spelled their way, e.g. "kudoo" / "coo doo" → "Kudu".
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryEntry {
    pub id: String,
    /// How the speech model tends to hear it. Matched case-insensitively as whole words.
    pub heard: Vec<String>,
    /// How it should be written.
    pub written: String,
    /// Learned automatically from a correction the user made. `None` in older files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub learned: Option<bool>,
}

impl DictionaryEntry {
    pub fn new(heard: Vec<String>, written: impl Into<String>, learned: bool) -> Self {
        Self { id: new_id(), heard, written: written.into(), learned: learned.then_some(true) }
    }

    pub fn is_learned(&self) -> bool {
        self.learned == Some(true)
    }
}

struct Rule {
    /// The alternatives without the word-edge look-arounds: a cheap check that skips the
    /// backtracking search for the (usual) texts that contain none of them.
    prefilter: regex::Regex,
    regex: fancy_regex::Regex,
    written: String,
}

/// Replaces misheard words with the user's spelling. Also fixes the capitalization of the
/// written form itself ("kudu" → "Kudu", "github" → "GitHub").
pub struct DictionaryStage {
    rules: Vec<Rule>,
}

impl DictionaryStage {
    pub fn new(entries: &[DictionaryEntry]) -> Self {
        let rules = entries
            .iter()
            .filter_map(|entry| {
                let mut variants: Vec<&str> = entry
                    .heard
                    .iter()
                    .chain(std::iter::once(&entry.written))
                    .map(|variant| trim_spaces(variant))
                    .filter(|variant| !variant.is_empty())
                    .collect();
                if variants.is_empty() || entry.written.is_empty() {
                    return None;
                }
                // Longest first so "coo doo app" wins over "coo doo".
                variants.sort_by_key(|variant| std::cmp::Reverse(char_count(variant)));
                let alternatives = |escape: fn(&str) -> String| {
                    variants.iter().map(|variant| escape(variant).replace(' ', r"[\s-]+")).collect::<Vec<_>>().join("|")
                };
                let prefilter = regex::Regex::new(&format!("(?i)(?:{})", alternatives(regex::escape))).ok()?;
                let pattern = format!(
                    r"(?i)(?<![\w'])(?:{})(?![\w'])",
                    alternatives(|text| fancy_regex::escape(text).into_owned())
                );
                let regex = fancy_regex::Regex::new(&pattern).ok()?;
                Some(Rule { prefilter, regex, written: entry.written.clone() })
            })
            .collect();
        Self { rules }
    }

    /// How many words in `text` this dictionary would actually correct (spelling that differs
    /// from the user's, not words that are already right).
    pub fn corrections(&self, text: &str) -> usize {
        self.rules
            .iter()
            .filter(|rule| rule.prefilter.is_match(text))
            .map(|rule| {
                rule.regex.find_iter(text).map_while(Result::ok).filter(|found| found.as_str() != rule.written).count()
            })
            .sum()
    }
}

impl TextStage for DictionaryStage {
    fn apply(&self, text: &str, _context: &TextContext) -> String {
        let mut result = text.to_owned();
        for rule in &self.rules {
            if !rule.prefilter.is_match(&result) {
                continue;
            }
            if let Ok(replaced) = rule.regex.try_replacen(&result, 0, NoExpand(&rule.written)) {
                result = replaced.into_owned();
            }
        }
        result
    }
}

impl std::fmt::Debug for DictionaryStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DictionaryStage({} rules)", self.rules.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(heard: &[&str], written: &str) -> DictionaryEntry {
        DictionaryEntry::new(heard.iter().map(|word| word.to_string()).collect(), written, false)
    }

    fn stage() -> DictionaryStage {
        DictionaryStage::new(&[
            entry(&["kudoo", "coo doo"], "Kudu"),
            entry(&["git hub"], "GitHub"),
            entry(&["post gress", "postgres"], "PostgreSQL"),
        ])
    }

    #[test]
    fn replaces() {
        let cases = [
            ("I pushed kudoo to git hub.", "I pushed Kudu to GitHub."),
            ("The coo doo app is fast.", "The Kudu app is fast."),
            ("Coo-doo rocks.", "Kudu rocks."),
            ("we moved to postgres", "we moved to PostgreSQL"),
            ("open github now", "open GitHub now"),
        ];
        let stage = stage();
        for (input, expected) in cases {
            assert_eq!(stage.apply(input, &TextContext::default()), expected, "{input}");
        }
    }

    #[test]
    fn matches_whole_words_only() {
        assert_eq!(stage().apply("Kudoos to the team", &TextContext::default()), "Kudoos to the team");
    }

    #[test]
    fn empty_dictionary_is_no_op() {
        assert_eq!(DictionaryStage::new(&[]).apply("anything", &TextContext::default()), "anything");
    }

    #[test]
    fn counts_only_real_corrections() {
        assert_eq!(stage().corrections("Kudu and kudoo and git hub"), 2);
    }

    #[test]
    fn replacement_is_literal() {
        let stage = DictionaryStage::new(&[entry(&["dollar one"], "$1 (a.b)")]);
        assert_eq!(stage.apply("pay dollar one now", &TextContext::default()), "pay $1 (a.b) now");
    }

    #[test]
    fn entries_use_the_mac_apps_json() {
        let json = r#"{"id":"E621E1F8-C36C-495A-93FC-0C247A3E6E5F","heard":["kudoo"],"written":"Kudu"}"#;
        let entry: DictionaryEntry = serde_json::from_str(json).unwrap();
        assert!(!entry.is_learned());
        assert_eq!(serde_json::to_string(&entry).unwrap(), json);
        let learned = DictionaryEntry::new(vec!["pia".into()], "Priya", true);
        assert_eq!(serde_json::to_value(&learned).unwrap()["learned"], true);
    }
}
