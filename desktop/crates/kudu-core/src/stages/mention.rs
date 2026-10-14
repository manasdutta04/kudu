use crate::app_category::AppCategory;
use crate::correction_learner::is_stopword;
use crate::pipeline::{TextContext, TextStage};
use crate::text::{lowercase, upper_first};
use regex::Regex;
use std::sync::LazyLock;

static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b[Tt]ag @?([A-Za-z][\w'’-]+)").unwrap());

#[rustfmt::skip]
const NOT_NAMES: &[&str] = &[
    "along", "team", "everyone", "everybody", "someone", "somebody", "people", "line", "name", "them", "him", "her",
    "me", "us", "all", "both", "each", "whoever", "anyone",
];

/// "tag Priya" becomes "@Priya", the way people mention someone in chat apps, documents and
/// their comments. Not in email, where there are no mentions, or in code.
#[derive(Clone, Copy, Debug, Default)]
pub struct MentionStage;

impl TextStage for MentionStage {
    fn apply(&self, text: &str, context: &TextContext) -> String {
        if matches!(context.category, AppCategory::Email | AppCategory::Code) {
            return text.to_owned();
        }
        let mut result = text.to_owned();
        let tags: Vec<_> =
            TAG.captures_iter(text).filter_map(|captures| Some((captures.get(0)?, captures.get(1)?))).collect();
        for (whole, name) in tags.into_iter().rev() {
            // "tag along", "tag me", "tag everyone in it": not a person's name.
            let lowercased = lowercase(name.as_str());
            if NOT_NAMES.contains(&lowercased.as_str()) || is_stopword(&lowercased) {
                continue;
            }
            result.replace_range(whole.range(), &format!("@{}", upper_first(name.as_str())));
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mentions_outside_chat_apps_too() {
        assert_eq!(MentionStage.apply("Can you tag Priya?", &TextContext::default()), "Can you @Priya?");
    }

    #[test]
    fn skips_words_that_are_not_names() {
        let context = TextContext::for_category(AppCategory::WorkMessage);
        assert_eq!(MentionStage.apply("Can I tag along?", &context), "Can I tag along?");
        assert_eq!(MentionStage.apply("Please tag me on it", &context), "Please tag me on it");
    }

    #[test]
    fn tags_become_mentions_in_chat_only() {
        let text = "Auth flow is done, tag priya, she should review it.";
        assert_eq!(
            MentionStage.apply(text, &TextContext::for_category(AppCategory::WorkMessage)),
            "Auth flow is done, @Priya, she should review it."
        );
        assert_eq!(MentionStage.apply(text, &TextContext::for_category(AppCategory::Email)), text);
    }
}
