//! The Insights page: daily usage totals and everything computed from them.

use crate::app_category::AppCategory;
use crate::calendar::{Calendar, Day};
use crate::usage_stats::DictationRecord;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

/// One day's usage. Totals only, no text, so it can be kept for good. Stored in `usage.json`
/// as a map from `"yyyy-MM-dd"` to this.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DayTotal {
    #[serde(deserialize_with = "null_as_default")]
    pub words: usize,
    #[serde(deserialize_with = "null_as_default")]
    pub dictations: usize,
    #[serde(deserialize_with = "null_as_default")]
    pub audio_seconds: f64,
    /// Words changed by cleanup: fillers, corrections, dictionary spellings.
    #[serde(deserialize_with = "null_as_default")]
    pub cleanups: usize,
    #[serde(deserialize_with = "null_as_default")]
    pub dictionary_fixes: usize,
    #[serde(deserialize_with = "null_as_default")]
    pub learned: usize,
    /// Words per app category ([`AppCategory::as_str`]).
    #[serde(deserialize_with = "null_as_default")]
    pub categories: HashMap<String, usize>,
    /// Words per app (bundle ID, executable or app id).
    #[serde(deserialize_with = "null_as_default")]
    pub apps: HashMap<String, usize>,
}

/// A missing or `null` field reads as its default, like Swift's `decodeIfPresent(…) ?? default`.
fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

impl DayTotal {
    /// Adds one dictation, made in an app of `category`, to the day's totals.
    pub fn add(&mut self, record: &DictationRecord, category: AppCategory) {
        self.words += record.words;
        self.dictations += 1;
        self.audio_seconds += record.audio_seconds;
        self.cleanups += record.cleanups.unwrap_or(0);
        self.dictionary_fixes += record.dictionary_fixes.unwrap_or(0);
        *self.categories.entry(category.as_str().to_owned()).or_default() += record.words;
        if let Some(app) = &record.bundle_id {
            *self.apps.entry(app.clone()).or_default() += record.words;
        }
    }
}

/// One day on the heat map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub date: Day,
    pub words: usize,
    /// 0 (none) … 4 (one of your busiest days).
    pub level: u8,
    /// Days after today, drawn empty.
    pub is_future: bool,
}

/// Everything the Insights page shows, computed from daily totals.
#[derive(Clone, Debug, PartialEq)]
pub struct Insights {
    pub total_words: usize,
    pub total_dictations: usize,
    pub words_per_minute: usize,
    pub cleanups: usize,
    pub dictionary_fixes: usize,
    pub learned: usize,
    pub current_streak: usize,
    pub longest_streak: usize,
    /// Change in words this month vs. the same days last month, as a fraction (0.093 = +9.3%).
    pub month_change: Option<f64>,
    /// Minutes saved against typing at [`Insights::TYPING_WORDS_PER_MINUTE`].
    pub minutes_saved: usize,
    /// Words per category, largest first.
    pub categories: Vec<(String, usize)>,
    /// Words per app, largest first.
    pub apps: Vec<(String, usize)>,
    /// Weeks (oldest first) of 7 days, Sunday first.
    pub heat_map: Vec<Vec<Cell>>,
}

impl Insights {
    pub const TYPING_WORDS_PER_MINUTE: f64 = 40.0;
    /// Weeks shown on the heat map.
    pub const DEFAULT_WEEKS: usize = 26;

    /// `days` are totals keyed by local day; `now` and `calendar` say which day is today.
    pub fn compute(days: &HashMap<Day, DayTotal>, now: SystemTime, weeks: usize, calendar: &Calendar) -> Self {
        let total_words: usize = days.values().map(|total| total.words).sum();
        let seconds: f64 = days.values().map(|total| total.audio_seconds).sum();
        let words_per_minute =
            if seconds >= 30.0 { (total_words as f64 / (seconds / 60.0)).round() as usize } else { 0 };

        let active: HashSet<Day> = days.iter().filter(|(_, total)| total.words > 0).map(|(day, _)| *day).collect();
        let today = calendar.day(now);

        let mut categories: HashMap<&str, usize> = HashMap::new();
        let mut apps: HashMap<&str, usize> = HashMap::new();
        for total in days.values() {
            for (category, words) in &total.categories {
                *categories.entry(category).or_default() += words;
            }
            for (app, words) in &total.apps {
                *apps.entry(app).or_default() += words;
            }
        }

        let typing_minutes = total_words as f64 / Self::TYPING_WORDS_PER_MINUTE;
        let minutes_saved = (typing_minutes - seconds / 60.0).round().max(0.0) as usize;

        Self {
            total_words,
            total_dictations: days.values().map(|total| total.dictations).sum(),
            words_per_minute,
            cleanups: days.values().map(|total| total.cleanups).sum(),
            dictionary_fixes: days.values().map(|total| total.dictionary_fixes).sum(),
            learned: days.values().map(|total| total.learned).sum(),
            current_streak: current_streak(&active, today),
            longest_streak: longest_streak(&active),
            month_change: month_change(days, today),
            minutes_saved,
            categories: largest_first(categories),
            apps: largest_first(apps),
            heat_map: heat_map(days, today, weeks),
        }
    }
}

/// Non-zero counts, largest first (ties by name, so the order is stable).
fn largest_first(counts: HashMap<&str, usize>) -> Vec<(String, usize)> {
    let mut sorted: Vec<(String, usize)> =
        counts.into_iter().filter(|(_, words)| *words > 0).map(|(name, words)| (name.to_owned(), words)).collect();
    sorted.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    sorted
}

fn current_streak(active: &HashSet<Day>, today: Day) -> usize {
    let mut day = today;
    if !active.contains(&day) {
        day = day.add_days(-1);
    }
    let mut streak = 0;
    while active.contains(&day) {
        streak += 1;
        day = day.add_days(-1);
    }
    streak
}

fn longest_streak(active: &HashSet<Day>) -> usize {
    let mut longest = 0;
    for &day in active {
        // Count only from the first day of each run.
        if active.contains(&day.add_days(-1)) {
            continue;
        }
        let mut length = 0;
        let mut cursor = day;
        while active.contains(&cursor) {
            length += 1;
            cursor = cursor.add_days(1);
        }
        longest = longest.max(length);
    }
    longest
}

/// This month so far vs. the same number of days at the start of last month.
fn month_change(days: &HashMap<Day, DayTotal>, today: Day) -> Option<f64> {
    let this_month = today.first_of_month();
    let last_month = this_month.add_months(-1);
    let elapsed = today.days_since(this_month) + 1;
    let last_month_end = last_month.add_days(elapsed);
    let words = |from: Day, to: Day| -> usize {
        days.iter().filter(|(day, _)| **day >= from && **day < to).map(|(_, total)| total.words).sum()
    };
    let current = words(this_month, today.add_days(1));
    let previous = words(last_month, last_month_end.min(this_month));
    if previous == 0 {
        return None;
    }
    Some((current as f64 - previous as f64) / previous as f64)
}

fn heat_map(days: &HashMap<Day, DayTotal>, today: Day, weeks: usize) -> Vec<Vec<Cell>> {
    // Levels are relative to your own busy days (the 90th percentile), so one huge day
    // doesn't wash everything else out.
    let mut counts: Vec<usize> = days.values().map(|total| total.words).filter(|&words| words > 0).collect();
    counts.sort_unstable();
    let reference =
        if counts.is_empty() { 1 } else { counts[(counts.len() - 1).min((counts.len() as f64 * 0.9) as usize)].max(1) };

    let this_sunday = today.add_days(-(today.weekday() as i32 - 1));
    let first_sunday = this_sunday.add_days(-(weeks as i32 - 1) * 7);

    (0..weeks)
        .map(|week| {
            (0..7)
                .map(|offset| {
                    let date = first_sunday.add_days((week * 7 + offset) as i32);
                    let words = days.get(&date).map_or(0, |total| total.words);
                    let level = if words == 0 {
                        0
                    } else {
                        4.min(1 + (words as f64 / reference as f64 * 3.999) as usize) as u8
                    };
                    Cell { date, words, level, is_future: date > today }
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::from_unix_seconds;

    // Wednesday 14 October 2026, 15:00 UTC
    fn now() -> SystemTime {
        from_unix_seconds(1_791_990_000, 0)
    }

    fn day(offset: i32) -> Day {
        Calendar::utc().day(now()).add_days(offset)
    }

    fn total(words: usize, seconds: f64, category: &str, app: &str) -> DayTotal {
        DayTotal {
            words,
            dictations: 1,
            audio_seconds: seconds,
            categories: HashMap::from([(category.to_owned(), words)]),
            apps: HashMap::from([(app.to_owned(), words)]),
            ..DayTotal::default()
        }
    }

    fn plain(words: usize) -> DayTotal {
        total(words, 60.0, "general", "notepad.exe")
    }

    fn compute(days: &HashMap<Day, DayTotal>, weeks: usize) -> Insights {
        Insights::compute(days, now(), weeks, &Calendar::utc())
    }

    #[test]
    fn streaks_across_a_gap() {
        let mut days = HashMap::new();
        for offset in [-9, -8, -7, -6, -5] {
            days.insert(day(offset), plain(50)); // 5-day run
        }
        for offset in [-2, -1, 0] {
            days.insert(day(offset), plain(50)); // current 3-day run
        }
        let insights = compute(&days, Insights::DEFAULT_WEEKS);
        assert_eq!(insights.longest_streak, 5);
        assert_eq!(insights.current_streak, 3);
    }

    #[test]
    fn heat_map_levels_and_shape() {
        let days = HashMap::from([(day(0), plain(400)), (day(-1), plain(100))]);
        let insights = compute(&days, 4);
        assert_eq!(insights.heat_map.len(), 4);
        assert!(insights.heat_map.iter().all(|week| week.len() == 7));
        let cells: Vec<&Cell> = insights.heat_map.iter().flatten().collect();
        let level = |date: Day| cells.iter().find(|cell| cell.date == date).map(|cell| cell.level);
        assert_eq!(level(day(0)), Some(4));
        assert_eq!(level(day(-1)), Some(1));
        assert_eq!(level(day(-2)), Some(0));
        assert!(cells.iter().any(|cell| cell.is_future)); // the rest of this week
        assert_eq!(insights.heat_map[0][0].date.weekday(), 1); // starts on Sunday
    }

    #[test]
    fn speed_shares_and_time_saved() {
        let days = HashMap::from([
            (day(0), total(240, 120.0, "email", "olk.exe")), // 120 wpm
            (day(-1), total(120, 60.0, "workMessage", "slack.exe")),
        ]);
        let insights = compute(&days, Insights::DEFAULT_WEEKS);
        assert_eq!(insights.words_per_minute, 120);
        assert_eq!(insights.categories.first().map(|(category, _)| category.as_str()), Some("email"));
        assert_eq!(insights.apps.first().map(|(app, _)| app.as_str()), Some("olk.exe"));
        assert_eq!(insights.minutes_saved, 6); // 360 words: 9 min typing − 3 min speaking
    }

    #[test]
    fn month_over_month_change() {
        let days = HashMap::from([
            (Day::from_ymd(2026, 10, 3).unwrap(), plain(110)),
            (Day::from_ymd(2026, 9, 3).unwrap(), plain(100)),
            (Day::from_ymd(2026, 9, 20).unwrap(), plain(999)), // after the same period
        ]);
        let change = compute(&days, Insights::DEFAULT_WEEKS).month_change;
        assert!(change.is_some_and(|change| (change - 0.1).abs() < 0.0001), "{change:?}");
    }

    #[test]
    fn day_totals_decode_like_the_mac_app() {
        let stored: HashMap<Day, DayTotal> = serde_json::from_str(
            r#"{"2026-10-14":{"words":12,"dictations":2,"audioSeconds":null,"apps":{"slack.exe":12}}}"#,
        )
        .unwrap();
        let total = &stored[&Day::from_ymd(2026, 10, 14).unwrap()];
        assert_eq!(total.words, 12);
        assert_eq!(total.audio_seconds, 0.0);
        assert_eq!(total.learned, 0);
        assert_eq!(total.apps["slack.exe"], 12);

        let mut added = DayTotal::default();
        added.add(
            &DictationRecord::new("hi", "Hi there.", 2.0, 100).with_bundle_id("slack.exe"),
            AppCategory::WorkMessage,
        );
        assert_eq!(added.categories["workMessage"], 2);
        let json = serde_json::to_value(&added).unwrap();
        assert_eq!(json["dictionaryFixes"], 0);
        assert_eq!(json["audioSeconds"], 2.0);
    }
}
