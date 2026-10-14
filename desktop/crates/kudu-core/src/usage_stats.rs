//! The on-device dictation history record and the headline numbers computed from it.

use crate::calendar::{iso8601, Calendar};
use crate::formatting_guard::{edit_distance, words};
use crate::id::new_id;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::SystemTime;

/// One finished dictation, as kept in the on-device history (`history.jsonl`, one JSON object
/// per line, same fields as the macOS app).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DictationRecord {
    pub id: String,
    /// When it was dictated; stored as ISO-8601 (`2026-10-14T15:00:00Z`).
    #[serde(with = "iso8601")]
    pub date: SystemTime,
    /// Exactly what the speech model returned, before cleanup.
    pub raw: String,
    /// What was inserted.
    pub text: String,
    pub words: usize,
    /// Length of the recording.
    pub audio_seconds: f64,
    /// Key release → text inserted.
    pub latency_ms: u64,
    /// The app the text went to (bundle ID on macOS, executable or app id on Windows and Linux).
    #[serde(rename = "bundleID", default, skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<String>,
    /// Whether smart formatting (the on-device language model) ran.
    pub formatted: bool,
    /// A Command mode result (an edit or draft) rather than a dictation. `None` in older records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<bool>,
    /// Words changed by cleanup between the raw transcript and the final text (fillers removed,
    /// corrections, dictionary spellings). `None` in older records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanups: Option<usize>,
    /// Dictionary spellings applied. `None` in older records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dictionary_fixes: Option<usize>,
}

impl DictationRecord {
    /// A dictation made now, with a new ID; the word count and cleanups are computed from the
    /// texts. Adjust the rest with the `with_…` methods.
    pub fn new(raw: impl Into<String>, text: impl Into<String>, audio_seconds: f64, latency_ms: u64) -> Self {
        let raw = raw.into();
        let text = text.into();
        Self {
            id: new_id(),
            date: SystemTime::now(),
            words: word_count(&text),
            cleanups: Some(edit_distance(&words(&raw), &words(&text))),
            raw,
            text,
            audio_seconds,
            latency_ms,
            bundle_id: None,
            formatted: false,
            command: None,
            dictionary_fixes: Some(0),
        }
    }

    pub fn with_date(mut self, date: SystemTime) -> Self {
        self.date = date;
        self
    }

    pub fn with_bundle_id(mut self, bundle_id: impl Into<String>) -> Self {
        self.bundle_id = Some(bundle_id.into());
        self
    }

    pub fn with_formatted(mut self, formatted: bool) -> Self {
        self.formatted = formatted;
        self
    }

    pub fn with_dictionary_fixes(mut self, fixes: usize) -> Self {
        self.dictionary_fixes = Some(fixes);
        self
    }

    /// Marks it as a Command mode result, which doesn't count cleanups.
    pub fn as_command(mut self) -> Self {
        self.command = Some(true);
        self.cleanups = Some(0);
        self
    }

    pub fn is_command(&self) -> bool {
        self.command == Some(true)
    }
}

/// Words as people count them: whitespace-separated tokens containing a letter or digit
/// ("—" and "-" alone don't count).
pub fn word_count(text: &str) -> usize {
    text.split_whitespace().filter(|token| token.chars().any(|c| c.is_alphabetic() || c.is_numeric())).count()
}

/// Headline numbers for the home screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UsageStats {
    pub total_words: usize,
    /// Speaking speed across dictations long enough to measure (≥ 1.5 s). 0 when unknown.
    pub words_per_minute: usize,
    /// Consecutive days with at least one dictation, ending today (or yesterday, so the
    /// streak doesn't read 0 first thing in the morning).
    pub day_streak: usize,
}

impl UsageStats {
    pub const EMPTY: UsageStats = UsageStats { total_words: 0, words_per_minute: 0, day_streak: 0 };

    pub fn compute(records: &[DictationRecord], now: SystemTime, calendar: &Calendar) -> Self {
        let total_words = records.iter().map(|record| record.words).sum();

        let timed = records.iter().filter(|record| record.audio_seconds >= 1.5);
        let (seconds, timed_words) =
            timed.fold((0.0, 0), |(seconds, words), record| (seconds + record.audio_seconds, words + record.words));
        let minutes = seconds / 60.0;
        let words_per_minute = if minutes > 0.0 { (timed_words as f64 / minutes).round() as usize } else { 0 };

        let days: HashSet<_> = records.iter().map(|record| calendar.day(record.date)).collect();
        let mut day = calendar.day(now);
        if !days.contains(&day) {
            day = day.add_days(-1);
        }
        let mut day_streak = 0;
        while days.contains(&day) {
            day_streak += 1;
            day = day.add_days(-1);
        }

        Self { total_words, words_per_minute, day_streak }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::from_unix_seconds;

    #[test]
    fn counts_words() {
        let cases = [("Hello there, world.", 3), ("Ship it — today", 3), ("  ", 0), ("Call me at 10:30 on 3 May", 7)];
        for (text, expected) in cases {
            assert_eq!(word_count(text), expected, "{text}");
        }
    }

    const NOW: i64 = 1_800_000_000; // a fixed afternoon (UTC)

    fn day(offset: i64, hour: i64) -> SystemTime {
        let start = NOW - NOW.rem_euclid(86_400);
        from_unix_seconds(start + offset * 86_400 + hour * 3600, 0)
    }

    fn record(date: SystemTime, words: usize, seconds: f64) -> DictationRecord {
        DictationRecord::new("", vec!["word"; words].join(" "), seconds, 100).with_date(date)
    }

    fn compute(records: &[DictationRecord]) -> UsageStats {
        UsageStats::compute(records, from_unix_seconds(NOW, 0), &Calendar::utc())
    }

    #[test]
    fn empty_history() {
        assert_eq!(compute(&[]), UsageStats { total_words: 0, words_per_minute: 0, day_streak: 0 });
    }

    #[test]
    fn totals_and_speed() {
        let records = [
            record(day(0, 12), 30, 15.0), // 120 wpm
            record(day(0, 12), 20, 10.0), // 120 wpm
            record(day(0, 12), 2, 0.8),   // too short to time
        ];
        let stats = compute(&records);
        assert_eq!(stats.total_words, 52);
        assert_eq!(stats.words_per_minute, 120);
    }

    #[test]
    fn streak_counts_consecutive_days_ending_today() {
        let records: Vec<_> = [0, -1, -2, -4].map(|offset| record(day(offset, 12), 5, 3.0)).into();
        assert_eq!(compute(&records).day_streak, 3);
    }

    #[test]
    fn streak_still_counts_before_todays_first_dictation() {
        let records: Vec<_> = [-1, -2].map(|offset| record(day(offset, 12), 5, 3.0)).into();
        assert_eq!(compute(&records).day_streak, 2);
    }

    #[test]
    fn streak_resets_after_a_missed_day() {
        let records: Vec<_> = [-2, -3].map(|offset| record(day(offset, 12), 5, 3.0)).into();
        assert_eq!(compute(&records).day_streak, 0);
    }

    #[test]
    fn records_use_the_mac_apps_json() {
        let record = DictationRecord::new("um send it", "Send it.", 2.5, 420)
            .with_date(from_unix_seconds(1_791_990_000, 0))
            .with_bundle_id("slack.exe");
        let json = serde_json::to_value(&record).unwrap();
        assert_eq!(json["date"], "2026-10-14T15:00:00Z");
        assert_eq!(json["bundleID"], "slack.exe");
        assert_eq!(json["latencyMs"], 420);
        assert_eq!(json["cleanups"], 1);
        assert!(json.get("command").is_none());

        let older = r#"{"id":"E621E1F8-C36C-495A-93FC-0C247A3E6E5F","date":"2026-10-14T15:00:00Z","raw":"hi","text":"Hi.","words":1,"audioSeconds":1.2,"latencyMs":300,"formatted":false}"#;
        let decoded: DictationRecord = serde_json::from_str(older).unwrap();
        assert_eq!(decoded.cleanups, None);
        assert_eq!(decoded.bundle_id, None);
        assert!(!decoded.is_command());
        assert_eq!(decoded.date, from_unix_seconds(1_791_990_000, 0));
    }
}
