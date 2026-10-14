//! Writing style per category of app (the Style page) and the stage that applies it.

use crate::app_category::AppCategory;
use crate::pipeline::{TextContext, TextStage};
use crate::usage_stats::word_count;
use serde::{Deserialize, Serialize};

/// How formal the text should read, chosen per category of app.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WritingStyle {
    /// Full sentences with capitals and punctuation.
    #[default]
    Formal,
    /// Normal capitals; short messages drop their final period.
    Casual,
    /// All lowercase, no final period.
    VeryCasual,
}

impl WritingStyle {
    pub const ALL: [WritingStyle; 3] = [Self::Formal, Self::Casual, Self::VeryCasual];

    /// The stored name (`"veryCasual"`).
    pub fn id(self) -> &'static str {
        match self {
            Self::Formal => "formal",
            Self::Casual => "casual",
            Self::VeryCasual => "veryCasual",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Formal => "Formal",
            Self::Casual => "Casual",
            Self::VeryCasual => "Very casual",
        }
    }

    pub fn example(self) -> &'static str {
        match self {
            Self::Formal => "Sounds good. See you at 3.",
            Self::Casual => "Sounds good. See you at 3",
            Self::VeryCasual => "sounds good. see you at 3",
        }
    }
}

/// The user's style for each category of app.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", from = "StoredStyleSettings")]
pub struct StyleSettings {
    pub personal_messages: WritingStyle,
    pub work_messages: WritingStyle,
    pub email: WritingStyle,
    pub other: WritingStyle,
}

impl Default for StyleSettings {
    fn default() -> Self {
        Self {
            personal_messages: WritingStyle::Casual,
            work_messages: WritingStyle::Casual,
            email: WritingStyle::Formal,
            other: WritingStyle::Formal,
        }
    }
}

impl StyleSettings {
    pub fn style_for(&self, category: AppCategory) -> WritingStyle {
        match category {
            AppCategory::PersonalMessage => self.personal_messages,
            AppCategory::WorkMessage => self.work_messages,
            AppCategory::Email => self.email,
            AppCategory::General | AppCategory::Code => self.other,
        }
    }
}

/// Settings saved before a field existed still load, with that field's default.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredStyleSettings {
    personal_messages: Option<WritingStyle>,
    work_messages: Option<WritingStyle>,
    email: Option<WritingStyle>,
    other: Option<WritingStyle>,
}

impl From<StoredStyleSettings> for StyleSettings {
    fn from(stored: StoredStyleSettings) -> Self {
        let defaults = Self::default();
        Self {
            personal_messages: stored.personal_messages.unwrap_or(defaults.personal_messages),
            work_messages: stored.work_messages.unwrap_or(defaults.work_messages),
            email: stored.email.unwrap_or(defaults.email),
            other: stored.other.unwrap_or(defaults.other),
        }
    }
}

/// Applies the writing style from the context. Runs after formatting, before cursor fitting.
#[derive(Clone, Copy, Debug, Default)]
pub struct StyleStage;

impl StyleStage {
    /// Messages up to this many words count as "short" for dropping the final period.
    pub const SHORT_MESSAGE_WORDS: usize = 30;
}

impl TextStage for StyleStage {
    fn apply(&self, text: &str, context: &TextContext) -> String {
        if context.category == AppCategory::Code || text.is_empty() {
            return text.to_owned();
        }
        match context.style {
            WritingStyle::Formal => text.to_owned(),
            WritingStyle::Casual => {
                if !context.category.is_message() || word_count(text) > Self::SHORT_MESSAGE_WORDS {
                    return text.to_owned();
                }
                dropping_final_period(text).to_owned()
            }
            WritingStyle::VeryCasual => dropping_final_period(&lowercasing_sentence_starts(text)).to_owned(),
        }
    }
}

/// "See you soon." → "See you soon". Keeps "?", "!" and "...".
fn dropping_final_period(text: &str) -> &str {
    if text.ends_with('.') && !text.ends_with("..") {
        &text[..text.len() - 1]
    } else {
        text
    }
}

/// Lowercases the first letter of each sentence, but not "I", acronyms ("API") or words with
/// capitals inside ("GitHub"), which are names.
fn lowercasing_sentence_starts(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut at_start = true;
    for (index, c) in text.char_indices() {
        if at_start && c.is_alphabetic() {
            let rest = &text[index..];
            let end = rest.find(|c: char| !(c.is_alphabetic() || c == '\'' || c == '’')).unwrap_or(rest.len());
            let word = &rest[..end];
            let keep = word == "I"
                || word.starts_with("I'")
                || word.starts_with("I’")
                || word.chars().skip(1).any(char::is_uppercase);
            if keep {
                result.push(c);
            } else {
                result.extend(c.to_lowercase());
            }
            at_start = false;
        } else {
            result.push(c);
            if matches!(c, '.' | '?' | '!' | '\n') {
                at_start = true;
            } else if !c.is_whitespace() {
                at_start = false;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(category: AppCategory, style: WritingStyle) -> TextContext {
        TextContext::new(category, None, style)
    }

    #[test]
    fn formal_leaves_text_alone() {
        let context = context(AppCategory::WorkMessage, WritingStyle::Formal);
        assert_eq!(StyleStage.apply("Sounds good. See you at 3.", &context), "Sounds good. See you at 3.");
    }

    #[test]
    fn casual_drops_final_period_of_short_messages() {
        let context = context(AppCategory::PersonalMessage, WritingStyle::Casual);
        assert_eq!(StyleStage.apply("Sounds good. See you at 3.", &context), "Sounds good. See you at 3");
        assert_eq!(StyleStage.apply("Are you coming?", &context), "Are you coming?");
    }

    #[test]
    fn casual_keeps_period_outside_messages_and_for_long_text() {
        assert_eq!(
            StyleStage.apply("See you at 3.", &context(AppCategory::Email, WritingStyle::Casual)),
            "See you at 3."
        );
        let long = vec!["word"; 40].join(" ") + ".";
        assert_eq!(StyleStage.apply(&long, &context(AppCategory::WorkMessage, WritingStyle::Casual)), long);
    }

    #[test]
    fn very_casual_lowercases_but_keeps_names() {
        let context = context(AppCategory::PersonalMessage, WritingStyle::VeryCasual);
        assert_eq!(
            StyleStage.apply("Sounds good. I pushed it to GitHub. NASA called.", &context),
            "sounds good. I pushed it to GitHub. NASA called"
        );
    }

    #[test]
    fn code_is_never_styled() {
        let context = context(AppCategory::Code, WritingStyle::VeryCasual);
        assert_eq!(StyleStage.apply("Run the tests.", &context), "Run the tests.");
    }

    #[test]
    fn style_settings_decode_like_the_mac_app() {
        let decoded: StyleSettings = serde_json::from_str(r#"{"email":"veryCasual"}"#).unwrap();
        assert_eq!(decoded, StyleSettings { email: WritingStyle::VeryCasual, ..StyleSettings::default() });
        assert_eq!(
            serde_json::to_string(&StyleSettings::default()).unwrap(),
            r#"{"personalMessages":"casual","workMessages":"casual","email":"formal","other":"formal"}"#
        );
        assert_eq!(StyleSettings::default().style_for(AppCategory::Code), WritingStyle::Formal);
    }
}
