//! Linux evdev key codes (`linux/input-event-codes.h`): which are modifiers, which type text,
//! and how each is labelled in shortcuts.

use kudu_core::shortcut::{Modifier, ModifierKind, Side};

pub const KEY_ESC: u16 = 1;
pub const KEY_TAB: u16 = 15;
pub const KEY_ENTER: u16 = 28;
pub const KEY_LEFTCTRL: u16 = 29;
pub const KEY_LEFTSHIFT: u16 = 42;
pub const KEY_C: u16 = 46;
pub const KEY_V: u16 = 47;
pub const KEY_LEFTALT: u16 = 56;
pub const KEY_SPACE: u16 = 57;
pub const KEY_RIGHTCTRL: u16 = 97;
pub const KEY_RIGHTSHIFT: u16 = 54;
pub const KEY_RIGHTALT: u16 = 100;
pub const KEY_LEFTMETA: u16 = 125;
pub const KEY_RIGHTMETA: u16 = 126;
pub const KEY_FN: u16 = 0x1d0;
pub const KEY_A: u16 = 30;
pub const KEY_KPENTER: u16 = 96;

/// The highest key code the virtual keyboard declares (the classic keyboard range; enough for
/// any key a layout maps letters to).
pub const VIRTUAL_KEY_MAX: u16 = 248;

/// The modifier a key is, with its side.
pub fn modifier_for(code: u16) -> Option<Modifier> {
    use ModifierKind::*;
    let (kind, side) = match code {
        KEY_LEFTCTRL => (Control, Side::Left),
        KEY_RIGHTCTRL => (Control, Side::Right),
        KEY_LEFTALT => (Alt, Side::Left),
        KEY_RIGHTALT => (Alt, Side::Right),
        KEY_LEFTSHIFT => (Shift, Side::Left),
        KEY_RIGHTSHIFT => (Shift, Side::Right),
        KEY_LEFTMETA => (Super, Side::Left),
        KEY_RIGHTMETA => (Super, Side::Right),
        KEY_FN => (Fn, Side::Any),
        _ => return None,
    };
    Some(Modifier::new(kind, side))
}

/// Letters in key-code order, row by row (Q…P, A…L, Z…M).
const LETTER_ROWS: [(u16, &str); 3] = [(16, "QWERTYUIOP"), (30, "ASDFGHJKL"), (44, "ZXCVBNM")];

fn letter(code: u16) -> Option<char> {
    LETTER_ROWS.iter().find_map(|(start, row)| {
        let index = code.checked_sub(*start)? as usize;
        row.chars().nth(index)
    })
}

fn digit(code: u16) -> Option<char> {
    match code {
        2..=10 => char::from_digit((code - 1) as u32, 10),
        11 => Some('0'),
        _ => None,
    }
}

/// Punctuation keys, labelled with their US-layout glyph.
fn punctuation(code: u16) -> Option<&'static str> {
    Some(match code {
        12 => "-",
        13 => "=",
        26 => "[",
        27 => "]",
        39 => ";",
        40 => "'",
        41 => "`",
        43 => "\\",
        51 => ",",
        52 => ".",
        53 => "/",
        86 => "<", // the extra key of ISO keyboards
        _ => return None,
    })
}

/// Keypad keys that type a character (with Num Lock on).
fn keypad(code: u16) -> Option<&'static str> {
    Some(match code {
        71 => "Num 7",
        72 => "Num 8",
        73 => "Num 9",
        74 => "Num -",
        75 => "Num 4",
        76 => "Num 5",
        77 => "Num 6",
        78 => "Num +",
        79 => "Num 1",
        80 => "Num 2",
        81 => "Num 3",
        82 => "Num 0",
        83 => "Num .",
        55 => "Num *",
        98 => "Num /",
        117 => "Num =",
        121 => "Num ,",
        _ => return None,
    })
}

fn function_key(code: u16) -> Option<u16> {
    match code {
        59..=68 => Some(code - 58),        // F1–F10
        87 => Some(11),                    // F11
        88 => Some(12),                    // F12
        183..=194 => Some(code - 183 + 13), // F13–F24
        _ => None,
    }
}

/// Whether a key types text, so it can't be held on its own to dictate.
pub fn key_types_text(code: u16) -> bool {
    letter(code).is_some()
        || digit(code).is_some()
        || punctuation(code).is_some()
        || keypad(code).is_some()
        || matches!(code, KEY_SPACE | KEY_ENTER | KEY_KPENTER | KEY_TAB)
}

/// What a key shows in a shortcut: "V", "F9", "Space", "/".
pub fn key_label(code: u16) -> String {
    if let Some(c) = letter(code).or_else(|| digit(code)) {
        return c.to_string();
    }
    if let Some(n) = function_key(code) {
        return format!("F{n}");
    }
    if let Some(glyph) = punctuation(code).or_else(|| keypad(code)) {
        return glyph.to_string();
    }
    let name = match code {
        KEY_ESC => "Esc",
        14 => "Backspace",
        KEY_TAB => "Tab",
        KEY_ENTER => "Enter",
        KEY_SPACE => "Space",
        58 => "Caps Lock",
        69 => "Num Lock",
        70 => "Scroll Lock",
        KEY_KPENTER => "Num Enter",
        99 => "Print Screen",
        102 => "Home",
        103 => "Up",
        104 => "Page Up",
        105 => "Left",
        106 => "Right",
        107 => "End",
        108 => "Down",
        109 => "Page Down",
        110 => "Insert",
        111 => "Delete",
        113 => "Mute",
        114 => "Volume Down",
        115 => "Volume Up",
        119 => "Pause",
        127 | 139 => "Menu",
        163 => "Next Track",
        164 => "Play/Pause",
        165 => "Previous Track",
        KEY_FN => "fn",
        _ => return format!("Key {code}"),
    };
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_keys_like_a_us_keyboard() {
        assert_eq!(key_label(KEY_V), "V");
        assert_eq!(key_label(KEY_C), "C");
        assert_eq!(key_label(KEY_A), "A");
        assert_eq!(key_label(16), "Q");
        assert_eq!(key_label(50), "M");
        assert_eq!(key_label(2), "1");
        assert_eq!(key_label(10), "9");
        assert_eq!(key_label(11), "0");
        assert_eq!(key_label(59), "F1");
        assert_eq!(key_label(68), "F10");
        assert_eq!(key_label(87), "F11");
        assert_eq!(key_label(88), "F12");
        assert_eq!(key_label(183), "F13");
        assert_eq!(key_label(194), "F24");
        assert_eq!(key_label(KEY_SPACE), "Space");
        assert_eq!(key_label(53), "/");
        assert_eq!(key_label(43), "\\");
        assert_eq!(key_label(103), "Up");
        assert_eq!(key_label(999), "Key 999");
    }

    #[test]
    fn knows_which_keys_type_text() {
        for code in [KEY_V, KEY_A, 2, 11, 12, 53, KEY_SPACE, KEY_ENTER, 79] {
            assert!(key_types_text(code), "{code} types");
        }
        for code in [59, 88, 183, 119, 110, KEY_ESC, KEY_LEFTCTRL, 103] {
            assert!(!key_types_text(code), "{code} doesn't type");
        }
    }

    #[test]
    fn maps_modifiers_with_their_side() {
        assert_eq!(modifier_for(KEY_LEFTCTRL), Some(Modifier::new(ModifierKind::Control, Side::Left)));
        assert_eq!(modifier_for(KEY_RIGHTALT), Some(Modifier::new(ModifierKind::Alt, Side::Right)));
        assert_eq!(modifier_for(KEY_LEFTMETA), Some(Modifier::new(ModifierKind::Super, Side::Left)));
        assert_eq!(modifier_for(KEY_RIGHTSHIFT), Some(Modifier::new(ModifierKind::Shift, Side::Right)));
        assert_eq!(modifier_for(KEY_FN), Some(Modifier::any(ModifierKind::Fn)));
        assert_eq!(modifier_for(KEY_V), None);
    }
}
