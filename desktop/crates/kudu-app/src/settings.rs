//! App settings (`settings.json`). Text cleanup, the dictionary and snippets live in their own
//! files with the same format as the Mac app's, so they can be copied between computers.

use kudu_core::shortcut::Shortcuts;
use kudu_engine::{SmartModel, SpeechModel};
use serde::{Deserialize, Serialize};

/// How the dictation shortcut starts and stops recording.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecordingMode {
    /// Hold to talk; double-tap to record hands-free, tap again to finish.
    #[default]
    HoldOrDoubleTap,
    /// Tap once to start, tap again to finish. Holding still works as push-to-talk.
    TapToToggle,
    /// Hold to talk only.
    HoldOnly,
}

impl RecordingMode {
    pub const ALL: [RecordingMode; 3] = [RecordingMode::HoldOrDoubleTap, RecordingMode::TapToToggle, RecordingMode::HoldOnly];

    pub fn title(self) -> &'static str {
        match self {
            RecordingMode::HoldOrDoubleTap => "Hold, or double-tap for hands-free",
            RecordingMode::TapToToggle => "Tap to start, tap to stop",
            RecordingMode::HoldOnly => "Hold to talk only",
        }
    }

    pub fn hint(self, key: &str) -> String {
        match self {
            RecordingMode::HoldOrDoubleTap => format!("Hold {key} and speak, or double-tap it to talk hands-free."),
            RecordingMode::TapToToggle => format!("Tap {key} to start, tap again to finish."),
            RecordingMode::HoldOnly => format!("Hold {key}, speak, release."),
        }
    }
}

/// How long dictation history is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Retention {
    ThirtyDays,
    #[default]
    NinetyDays,
    Forever,
}

impl Retention {
    pub const ALL: [Retention; 3] = [Retention::ThirtyDays, Retention::NinetyDays, Retention::Forever];

    pub fn title(self) -> &'static str {
        match self {
            Retention::ThirtyDays => "30 days",
            Retention::NinetyDays => "90 days",
            Retention::Forever => "Forever",
        }
    }

    pub fn days(self) -> Option<u64> {
        match self {
            Retention::ThirtyDays => Some(30),
            Retention::NinetyDays => Some(90),
            Retention::Forever => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// `None` until first launch picks the platform defaults.
    pub shortcuts: Option<Shortcuts>,
    pub recording_mode: RecordingMode,
    /// Microphone name; `None` follows the system default.
    pub microphone: Option<String>,
    pub speech_model: SpeechModel,
    /// The downloaded language model for smart formatting and Command mode, if any.
    pub smart_model: Option<SmartModel>,
    pub sounds: bool,
    pub learn_from_corrections: bool,
    /// Read the names and terms in the window you're dictating into, to spell them right.
    pub screen_context: bool,
    pub retention: Retention,
    /// Setup was finished once (the window stops showing the welcome card).
    pub onboarded: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            shortcuts: None,
            recording_mode: RecordingMode::default(),
            microphone: None,
            speech_model: SpeechModel::Parakeet,
            smart_model: None,
            sounds: true,
            learn_from_corrections: true,
            screen_context: true,
            retention: Retention::default(),
            onboarded: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_older_or_partial_files() {
        let settings: Settings = serde_json::from_str(r#"{"sounds": false}"#).unwrap();
        assert!(!settings.sounds);
        assert_eq!(settings.speech_model, SpeechModel::Parakeet);
        let round_trip: Settings = serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(round_trip, settings);
    }
}
