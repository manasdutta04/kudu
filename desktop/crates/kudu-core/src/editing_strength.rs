//! How much the on-device language model may change a dictation.

use serde::{Deserialize, Serialize};

/// How much the on-device model may change a dictation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditingStrength {
    /// Rules only: exactly what you said, with light cleanup.
    Verbatim,
    /// Punctuation, paragraphs, lists and number style; your words stay as spoken.
    #[default]
    Light,
    /// Also smooths false starts, repetitions and rambling phrasing.
    Polished,
}

impl EditingStrength {
    pub const ALL: [EditingStrength; 3] = [Self::Verbatim, Self::Light, Self::Polished];

    /// The stored name (`"polished"`).
    pub fn id(self) -> &'static str {
        match self {
            Self::Verbatim => "verbatim",
            Self::Light => "light",
            Self::Polished => "polished",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Verbatim => "Verbatim",
            Self::Light => "Light",
            Self::Polished => "Polished",
        }
    }

    pub fn summary(self) -> &'static str {
        match self {
            Self::Verbatim => "Exactly what you said, with fillers removed. Fastest.",
            Self::Light => "Adds punctuation, paragraphs, lists and numbers. Keeps your words.",
            Self::Polished => "Also smooths false starts and repetition for cleaner prose.",
        }
    }

    /// Largest share of words the model may change.
    pub fn maximum_word_change(self) -> f64 {
        if self == Self::Polished {
            0.5
        } else {
            0.35
        }
    }

    /// Largest change in length the model may make.
    pub fn maximum_length_change(self) -> f64 {
        if self == Self::Polished {
            0.35
        } else {
            0.2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_names_match_the_mac_app() {
        for strength in EditingStrength::ALL {
            assert_eq!(serde_json::to_string(&strength).unwrap(), format!("\"{}\"", strength.id()));
        }
    }
}
