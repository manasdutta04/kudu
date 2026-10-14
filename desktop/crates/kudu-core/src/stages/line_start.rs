use crate::pipeline::{TextContext, TextStage};

/// Capitalizes the first letter of every line after the first ("Dear Priya,\nthanks…" →
/// "Thanks…"). The first line is left to [`CursorFitter`](super::CursorFitter), which knows
/// whether the text continues a sentence already in the field.
#[derive(Clone, Copy, Debug, Default)]
pub struct LineStartCapitalizer;

impl TextStage for LineStartCapitalizer {
    fn apply(&self, text: &str, _context: &TextContext) -> String {
        if !text.contains('\n') {
            return text.to_owned();
        }
        let mut result = String::with_capacity(text.len());
        let mut at_line_start = false;
        for c in text.chars() {
            if at_line_start && c.is_alphabetic() {
                result.extend(c.to_uppercase());
                at_line_start = false;
                continue;
            }
            result.push(c);
            if c == '\n' {
                at_line_start = true;
            } else if !c.is_whitespace() && !(at_line_start && (c.is_numeric() || matches!(c, '.' | '-' | '•' | ')')))
            {
                at_line_start = false;
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(text: &str) -> String {
        LineStartCapitalizer.apply(text, &TextContext::default())
    }

    #[test]
    fn capitalizes_each_new_line() {
        assert_eq!(
            apply("Dear Priya,\nthanks for this.\n\nbest,\nManas"),
            "Dear Priya,\nThanks for this.\n\nBest,\nManas"
        );
    }

    #[test]
    fn handles_numbered_list_items() {
        assert_eq!(apply("We need:\n1. the page\n2. the email"), "We need:\n1. The page\n2. The email");
    }

    #[test]
    fn leaves_first_line_and_single_lines_alone() {
        assert_eq!(apply("and then more"), "and then more");
        assert_eq!(apply("and then\nmore"), "and then\nMore");
    }
}
