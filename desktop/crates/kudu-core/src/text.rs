//! Character classes and small string helpers with the meaning of the Foundation APIs the
//! macOS app uses (`Character.isPunctuation`, `CharacterSet.whitespaces`, `lowercased()`…),
//! so ported logic behaves the same on every platform.
//!
//! `char::is_alphabetic`, `is_numeric`, `is_uppercase`, `is_lowercase` and `is_whitespace`
//! already match Swift's `Character.isLetter`, `isNumber`, `isUppercase`, `isLowercase` and
//! `isWhitespace` for ordinary text, so they are used directly.

use regex::Regex;
use std::sync::LazyLock;

/// ICU's `.`: any character except a line terminator (`\n`, `\x0B`, `\x0C`, `\r`, NEL, LS, PS).
/// Rust's `.` only stops at `\n`, so patterns ported from `NSRegularExpression` use this.
pub(crate) const ICU_DOT: &str = r"[^\n\x0B\x0C\r\x{85}\x{2028}\x{2029}]";

static NON_ASCII_PUNCTUATION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\A\p{P}\z").unwrap());
static NON_ASCII_SYMBOL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\A\p{S}\z").unwrap());

/// Unicode general category P* (`Character.isPunctuation`, `CharacterSet.punctuationCharacters`).
pub(crate) fn is_punctuation(c: char) -> bool {
    if c.is_ascii() {
        return matches!(
            c,
            '!' | '"'
                | '#'
                | '%'
                | '&'
                | '\''
                | '('
                | ')'
                | '*'
                | ','
                | '-'
                | '.'
                | '/'
                | ':'
                | ';'
                | '?'
                | '@'
                | '['
                | '\\'
                | ']'
                | '_'
                | '{'
                | '}'
        );
    }
    let mut buffer = [0; 4];
    NON_ASCII_PUNCTUATION.is_match(c.encode_utf8(&mut buffer))
}

/// Unicode general category S* (`CharacterSet.symbols`).
pub(crate) fn is_symbol(c: char) -> bool {
    if c.is_ascii() {
        return matches!(c, '$' | '+' | '<' | '=' | '>' | '^' | '`' | '|' | '~');
    }
    let mut buffer = [0; 4];
    NON_ASCII_SYMBOL.is_match(c.encode_utf8(&mut buffer))
}

/// `CharacterSet.whitespaces`: spaces (general category Zs) and tab, but not line breaks.
pub(crate) fn is_space(c: char) -> bool {
    matches!(c, '\t' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}')
}

/// Trims `CharacterSet.whitespaces` (spaces and tabs, not line breaks) from both ends.
/// (`str::trim` is the equivalent of `.whitespacesAndNewlines`.)
pub(crate) fn trim_spaces(text: &str) -> &str {
    text.trim_matches(is_space)
}

/// Swift's `lowercased()`: each character mapped on its own, without the context-dependent
/// final-sigma rule of `str::to_lowercase`.
pub(crate) fn lowercase(text: &str) -> String {
    if text.is_ascii() {
        return text.to_ascii_lowercase();
    }
    text.chars().flat_map(char::to_lowercase).collect()
}

/// The first character uppercased, the rest as is (`prefix(1).uppercased() + dropFirst()`).
pub(crate) fn upper_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The first character lowercased, the rest as is.
pub(crate) fn lower_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Length in characters (Swift's `count`).
pub(crate) fn char_count(text: &str) -> usize {
    text.chars().count()
}

/// The lowercased letters of a text, nothing else: "Git-Hub 2" → "github".
pub(crate) fn letters(text: &str) -> Vec<char> {
    text.chars().flat_map(char::to_lowercase).filter(|c| c.is_alphabetic()).collect()
}

/// Where ICU's `$` (without multi-line mode) can match besides the very end: before a single
/// line terminator that ends the text.
pub(crate) fn end_before_final_line_break(text: &str) -> usize {
    if text.ends_with("\r\n") {
        return text.len() - 2;
    }
    match text.chars().next_back() {
        Some(c @ ('\n' | '\x0B' | '\x0C' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}')) => text.len() - c.len_utf8(),
        _ => text.len(),
    }
}

/// Runs a look-around regex replacement; a runtime error (which needs pathological input to
/// hit the backtracking limit) leaves the text unchanged rather than panicking.
pub(crate) fn fancy_replace_all(regex: &fancy_regex::Regex, text: &str, replacement: &str) -> String {
    regex.try_replacen(text, 0, replacement).map(|replaced| replaced.into_owned()).unwrap_or_else(|_| text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_characters() {
        assert!(is_punctuation(','));
        assert!(is_punctuation('…'));
        assert!(is_punctuation('“'));
        assert!(!is_punctuation('$'));
        assert!(is_symbol('$'));
        assert!(is_symbol('€'));
        assert!(!is_symbol('a'));
        assert!(is_space('\u{A0}'));
        assert!(!is_space('\n'));
    }

    #[test]
    fn transforms_strings() {
        assert_eq!(trim_spaces("\t a b \n "), "a b \n");
        assert_eq!(upper_first("éa"), "Éa");
        assert_eq!(lower_first("Hello"), "hello");
        assert_eq!(letters("Git-Hub 2"), "github".chars().collect::<Vec<_>>());
        assert_eq!(lowercase("ΟΔΟΣ"), "οδοσ");
        assert_eq!(end_before_final_line_break("a\r\n"), 1);
        assert_eq!(end_before_final_line_break("a"), 1);
    }
}
