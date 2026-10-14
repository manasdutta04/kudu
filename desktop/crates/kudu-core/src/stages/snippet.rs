use crate::calendar::Calendar;
use crate::id::new_id;
use crate::pipeline::{TextContext, TextStage};
use crate::text::lowercase;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use std::time::SystemTime;

/// A spoken shortcut that expands to saved text, e.g. "my email" → "me@example.com".
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub id: String,
    pub trigger: String,
    /// May contain `{date}`, `{time}` and `{clipboard}` placeholders.
    pub expansion: String,
}

impl Snippet {
    pub fn new(trigger: impl Into<String>, expansion: impl Into<String>) -> Self {
        Self { id: new_id(), trigger: trigger.into(), expansion: expansion.into() }
    }
}

/// Reads the clipboard's text, if any.
pub type ClipboardFn = Arc<dyn Fn() -> Option<String> + Send + Sync>;
/// The current time.
pub type NowFn = Arc<dyn Fn() -> SystemTime + Send + Sync>;

/// Where snippet placeholders get their values: `{clipboard}` from `clipboard`, `{date}` and
/// `{time}` from `now` in the `calendar`'s time zone. The default has an empty clipboard, the
/// system clock and UTC; the app passes the local time zone.
#[derive(Clone)]
pub struct Placeholders {
    pub clipboard: ClipboardFn,
    pub now: NowFn,
    pub calendar: Calendar,
}

impl Default for Placeholders {
    fn default() -> Self {
        Self { clipboard: Arc::new(|| None), now: Arc::new(SystemTime::now), calendar: Calendar::utc() }
    }
}

impl std::fmt::Debug for Placeholders {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Placeholders").field("calendar", &self.calendar).finish_non_exhaustive()
    }
}

/// Everything but letters, marks, digits, spaces and tabs (`CharacterSet.alphanumerics ∪
/// .whitespaces`, inverted).
static NOT_KEY_CHARACTER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^\p{L}\p{M}\p{N}\p{Zs}\t]").unwrap());

/// Expands a snippet when the whole dictation is its trigger ("My address." → the address).
/// Matching the whole utterance keeps ordinary sentences that contain the words safe.
pub struct SnippetStage {
    snippets: HashMap<String, String>,
    placeholders: Placeholders,
}

impl SnippetStage {
    pub fn new(snippets: &[Snippet], placeholders: Placeholders) -> Self {
        let snippets = snippets
            .iter()
            .filter(|snippet| !snippet.trigger.is_empty())
            .map(|snippet| (key(&snippet.trigger), snippet.expansion.clone()))
            .collect();
        Self { snippets, placeholders }
    }

    /// The expansion if the whole dictation is a snippet trigger, otherwise `None`.
    pub fn expansion_for(&self, text: &str) -> Option<String> {
        if self.snippets.is_empty() {
            return None;
        }
        self.snippets.get(&key(text)).map(|expansion| self.fill(expansion))
    }

    fn fill(&self, template: &str) -> String {
        if !template.contains('{') {
            return template.to_owned();
        }
        let now = (self.placeholders.now)();
        let mut result = template.to_owned();
        if result.contains("{date}") {
            result = result.replace("{date}", &long_date(now, &self.placeholders.calendar));
        }
        if result.contains("{time}") {
            result = result.replace("{time}", &short_time(now, &self.placeholders.calendar));
        }
        if result.contains("{clipboard}") {
            result = result.replace("{clipboard}", &(self.placeholders.clipboard)().unwrap_or_default());
        }
        result
    }
}

impl TextStage for SnippetStage {
    fn apply(&self, text: &str, _context: &TextContext) -> String {
        self.expansion_for(text).unwrap_or_else(|| text.to_owned())
    }
}

impl std::fmt::Debug for SnippetStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SnippetStage({} snippets)", self.snippets.len())
    }
}

/// "My Email." / "my email" / " my  email! " all map to "my email".
fn key(text: &str) -> String {
    let lowercased = lowercase(text);
    let kept = NOT_KEY_CHARACTER.replace_all(&lowercased, "");
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[rustfmt::skip]
const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November",
    "December",
];

/// "October 14, 2026" (the long date style in English).
fn long_date(now: SystemTime, calendar: &Calendar) -> String {
    let (year, month, day) = calendar.day(now).ymd();
    format!("{} {day}, {year}", MONTHS[month as usize - 1])
}

/// "3:05 PM" (the short time style in English).
fn short_time(now: SystemTime, calendar: &Calendar) -> String {
    let (_, seconds) = calendar.local(now);
    let (hour, minute) = (seconds / 3600, seconds / 60 % 60);
    let twelve_hour = if hour % 12 == 0 { 12 } else { hour % 12 };
    format!("{twelve_hour}:{minute:02} {}", if hour < 12 { "AM" } else { "PM" })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::from_unix_seconds;

    fn stage() -> SnippetStage {
        SnippetStage::new(
            &[
                Snippet::new("my email", "me@example.com"),
                Snippet::new("sign off", "Thanks,\nManas"),
                Snippet::new("paste link", "Link: {clipboard}"),
            ],
            Placeholders { clipboard: Arc::new(|| Some("https://example.com".into())), ..Placeholders::default() },
        )
    }

    #[test]
    fn expands_whole_utterance() {
        for input in ["My email.", "my email", "  My   Email! "] {
            assert_eq!(stage().apply(input, &TextContext::default()), "me@example.com", "{input:?}");
        }
    }

    #[test]
    fn multi_line_expansion() {
        assert_eq!(stage().apply("Sign off.", &TextContext::default()), "Thanks,\nManas");
    }

    #[test]
    fn clipboard_placeholder() {
        assert_eq!(stage().apply("Paste link.", &TextContext::default()), "Link: https://example.com");
    }

    #[test]
    fn trigger_inside_sentence_is_left_alone() {
        let text = "Please send it to my email tomorrow.";
        assert_eq!(stage().apply(text, &TextContext::default()), text);
    }

    #[test]
    fn date_placeholder() {
        let fixed = Placeholders { now: Arc::new(|| from_unix_seconds(0, 0)), ..Placeholders::default() };
        let dated = SnippetStage::new(&[Snippet::new("today", "{date}")], fixed);
        assert_eq!(dated.apply("Today.", &TextContext::default()), "January 1, 1970");
    }

    #[test]
    fn time_placeholder_uses_the_calendar() {
        let placeholders = Placeholders {
            now: Arc::new(|| from_unix_seconds(1_791_990_000, 0)), // 15:00 UTC
            calendar: Calendar::fixed_offset(5 * 3600 + 1800),
            ..Placeholders::default()
        };
        let stage = SnippetStage::new(&[Snippet::new("now", "{date} {time}")], placeholders);
        assert_eq!(stage.apply("Now.", &TextContext::default()), "October 14, 2026 8:30 PM");
    }

    #[test]
    fn snippets_use_the_mac_apps_json() {
        let json = r#"{"id":"E621E1F8-C36C-495A-93FC-0C247A3E6E5F","trigger":"my email","expansion":"me@example.com"}"#;
        let snippet: Snippet = serde_json::from_str(json).unwrap();
        assert_eq!(serde_json::to_string(&snippet).unwrap(), json);
    }
}
