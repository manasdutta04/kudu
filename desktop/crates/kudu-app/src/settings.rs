//! App settings (`settings.json`). Text cleanup, the dictionary and snippets live in their own
//! files with the same format as the Mac app's, so they can be copied between computers.

use kudu_core::shortcut::Shortcuts;
use kudu_engine::{Pack, SpeechModel, WriterModel};
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
    /// The model pack: a speech model and a writer, downloaded together. Read it with
    /// `pack()`, which also covers settings files from before packs existed.
    pub pack: Option<Pack>,
    /// Older versions' model choices ("parakeet" / "moonshine-base", and an optional "small" /
    /// "better" language model), read once to pick the matching pack, never written again.
    #[serde(rename = "speechModel", skip_serializing)]
    legacy_speech: Option<String>,
    #[serde(rename = "smartModel", skip_serializing)]
    legacy_smart: Option<String>,
    pub sounds: bool,
    pub learn_from_corrections: bool,
    /// Read the names and terms in the window you're dictating into, to spell them right.
    pub screen_context: bool,
    pub retention: Retention,
    /// Setup was finished once (the window stops showing the welcome card).
    pub onboarded: bool,
    /// Check once a day whether a newer Kudu has been released.
    pub check_updates: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            shortcuts: None,
            recording_mode: RecordingMode::default(),
            microphone: None,
            pack: None,
            legacy_speech: None,
            legacy_smart: None,
            sounds: true,
            learn_from_corrections: true,
            screen_context: true,
            retention: Retention::default(),
            onboarded: false,
            check_updates: true,
        }
    }
}

impl Settings {
    /// The chosen pack. Settings from before packs map to the closest one: the compact speech
    /// model → kudu-tiny, the larger language model → kudu-large, anything else → kudu-base.
    pub fn pack(&self) -> Pack {
        self.pack.unwrap_or(match (self.legacy_speech.as_deref(), self.legacy_smart.as_deref()) {
            (Some("moonshine-base"), _) => Pack::Tiny,
            (_, Some("better")) => Pack::Large,
            _ => Pack::default(),
        })
    }

    pub fn speech(&self) -> SpeechModel {
        self.pack().speech()
    }

    pub fn writer(&self) -> WriterModel {
        self.pack().writer()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_older_or_partial_files() {
        let settings: Settings = serde_json::from_str(r#"{"sounds": false}"#).unwrap();
        assert!(!settings.sounds);
        assert_eq!(settings.pack(), Pack::Base);
        assert_eq!(settings.speech(), SpeechModel::Parakeet);
        let round_trip: Settings = serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(round_trip, settings);
    }

    #[test]
    fn older_model_choices_become_packs() {
        let pack = |json: &str| serde_json::from_str::<Settings>(json).unwrap().pack();
        assert_eq!(pack(r#"{"speechModel": "moonshine-base", "smartModel": "small"}"#), Pack::Tiny);
        assert_eq!(pack(r#"{"speechModel": "parakeet", "smartModel": "better"}"#), Pack::Large);
        assert_eq!(pack(r#"{"speechModel": "parakeet", "smartModel": "small"}"#), Pack::Base);
        assert_eq!(pack(r#"{"speechModel": "parakeet", "smartModel": null}"#), Pack::Base);
        assert_eq!(pack(r#"{"pack": "kudu-large", "speechModel": "moonshine-base"}"#), Pack::Large);
    }

    #[test]
    fn the_old_model_keys_are_not_written_back() {
        let old: Settings = serde_json::from_str(r#"{"speechModel": "moonshine-base", "smartModel": "better"}"#).unwrap();
        let mut migrated = old.clone();
        migrated.pack = Some(old.pack());
        let json = serde_json::to_string(&migrated).unwrap();
        assert!(json.contains(r#""pack":"kudu-tiny""#), "{json}");
        assert!(!json.contains("speechModel") && !json.contains("smartModel"), "{json}");
    }
}
