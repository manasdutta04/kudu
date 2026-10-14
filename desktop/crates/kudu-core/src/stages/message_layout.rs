use crate::app_category::AppCategory;
use crate::pipeline::{TextContext, TextStage};
use crate::text::{end_before_final_line_break, lowercase, upper_first};
use std::sync::LazyLock;

const NAME: &str = r"([A-Z][\w'’-]+(?: [A-Z][\w'’-]+)?)";

// The greeting word is case-insensitive; the name must be capitalized, so "Hey team" or
// "Dear all of you" are left alone.
static FORMAL_GREETING: LazyLock<fancy_regex::Regex> =
    LazyLock::new(|| fancy_regex::Regex::new(&format!(r"^((?i:dear)) {NAME}[,.]?\s+(?=\S)")).unwrap());
static CASUAL_GREETING: LazyLock<fancy_regex::Regex> =
    LazyLock::new(|| fancy_regex::Regex::new(&format!(r"^((?i:hi|hello|hey)) {NAME}[,.]?\s+(?=\S)")).unwrap());

// Sign-offs end the text: matched against it up to a final line break, like ICU's `$`.
static FORMAL_SIGN_OFF: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r"(?i)[,.]?\s+(Best regards|Kind regards|Warm regards|Regards|Sincerely|Yours sincerely|Best wishes)[,.]?\s+([A-Za-z][\w'’-]+)[.!]?\z",
    )
    .unwrap()
});
static CASUAL_SIGN_OFF: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?i)[,.]?\s+(Thanks|Thank you|Cheers|Best)[,.]?\s+([A-Za-z][\w'’-]+)[.!]?\z").unwrap()
});

/// Lays out letter-style greetings and sign-offs without the language model:
/// "Dear Priya thanks for…" → "Dear Priya,\nThanks for…", and
/// "…patience. Best regards Manas." → "…patience.\n\nBest regards,\nManas".
/// "Dear …" and unambiguous sign-offs ("best regards", "sincerely") apply everywhere; the
/// casual ones ("Hi Sam", "Thanks, Manas") only in email, where they are clearly a letter.
#[derive(Clone, Copy, Debug, Default)]
pub struct MessageLayout;

impl TextStage for MessageLayout {
    fn apply(&self, text: &str, context: &TextContext) -> String {
        if context.category == AppCategory::Code {
            return text.to_owned();
        }
        let mut result = text.to_owned();
        let is_email = context.category == AppCategory::Email;

        let greetings: &[&fancy_regex::Regex] =
            if is_email { &[&FORMAL_GREETING, &CASUAL_GREETING] } else { &[&FORMAL_GREETING] };
        for regex in greetings {
            let found = regex.captures(&result).ok().flatten().and_then(|captures| {
                let (whole, greeting, who) = (captures.get(0)?, captures.get(1)?, captures.get(2)?);
                Some((whole.range(), format!("{} {},\n", capitalized(greeting.as_str()), who.as_str())))
            });
            if let Some((whole, line)) = found {
                result.replace_range(whole, &line);
                break;
            }
        }

        let sign_offs: &[&regex::Regex] =
            if is_email { &[&FORMAL_SIGN_OFF, &CASUAL_SIGN_OFF] } else { &[&FORMAL_SIGN_OFF] };
        for regex in sign_offs {
            let end = end_before_final_line_break(&result);
            let found = regex.captures(&result[..end]).and_then(|captures| {
                let (whole, phrase, who) = (captures.get(0)?, captures.get(1)?, captures.get(2)?);
                let before = &result[..whole.start()];
                let ending = if before.ends_with(['.', '?', '!']) { "" } else { "." };
                let sign_off = format!("{ending}\n\n{},\n{}", upper_first(phrase.as_str()), upper_first(who.as_str()));
                Some((whole.range(), sign_off))
            });
            if let Some((whole, sign_off)) = found {
                result.replace_range(whole, &sign_off);
                break;
            }
        }
        result
    }
}

/// "hEY" → "Hey" (Foundation's `capitalized`, for a single word).
fn capitalized(word: &str) -> String {
    upper_first(&lowercase(word))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formal_greeting_and_sign_off_everywhere() {
        let text = "Dear Priya thanks for the contract. We are ready to sign. Thanks again for your patience. Best regards manas.";
        assert_eq!(
            MessageLayout.apply(text, &TextContext::default()),
            "Dear Priya,\nthanks for the contract. We are ready to sign. Thanks again for your patience.\n\nBest regards,\nManas"
        );
    }

    #[test]
    fn casual_ones_only_in_email() {
        let text = "Hi Sam, the deck is attached. Thanks, Manas.";
        assert_eq!(MessageLayout.apply(text, &TextContext::for_category(AppCategory::WorkMessage)), text);
        assert_eq!(
            MessageLayout.apply(text, &TextContext::for_category(AppCategory::Email)),
            "Hi Sam,\nthe deck is attached.\n\nThanks,\nManas"
        );
    }

    #[test]
    fn ordinary_sentences_are_untouched() {
        let text = "Hey team, quick update: the build is green. Thanks.";
        assert_eq!(MessageLayout.apply(text, &TextContext::for_category(AppCategory::Email)), text);
    }

    #[test]
    fn greeting_needs_text_after_it() {
        assert_eq!(MessageLayout.apply("Dear Priya ", &TextContext::default()), "Dear Priya ");
        assert_eq!(MessageLayout.apply("Dear Priya Sam ", &TextContext::default()), "Dear Priya,\nSam ");
    }

    #[test]
    fn sign_off_before_a_final_line_break() {
        assert_eq!(
            MessageLayout.apply("See you then. Kind regards Manas\n", &TextContext::default()),
            "See you then.\n\nKind regards,\nManas\n"
        );
    }
}
