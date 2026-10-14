//! Sentence-start capitalization shared by several stages.

/// Uppercases the first letter of the text and of every sentence that follows ". ", "? ",
/// "! " or a line break. Leaves everything else untouched.
pub fn capitalizing_sentence_starts(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut at_sentence_start = true;
    for c in text.chars() {
        if at_sentence_start && c.is_alphabetic() {
            result.extend(c.to_uppercase());
            at_sentence_start = false;
            continue;
        }
        result.push(c);
        if matches!(c, '.' | '?' | '!' | '\n') {
            at_sentence_start = true;
        } else if !c.is_whitespace() {
            at_sentence_start = false;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capitalizes_each_sentence() {
        assert_eq!(capitalizing_sentence_starts("hi. how are you?\nfine"), "Hi. How are you?\nFine");
        assert_eq!(capitalizing_sentence_starts("3.30 is fine"), "3.30 is fine");
    }
}
