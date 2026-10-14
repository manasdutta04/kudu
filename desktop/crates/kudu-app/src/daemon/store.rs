//! The user's data as the background process sees it. The window edits the same files and
//! sends `Reload`; files are also re-read when their modification time changes.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::time::{Duration, SystemTime};

use kudu_core::{AppCategory, Calendar, CleanupSettings, Day, DayTotal, DictationRecord, DictionaryEntry, Snippet};

use crate::paths::{modified, read_json, write_json, Paths};
use crate::settings::Settings;

pub struct Store {
    pub paths: Paths,
    pub settings: Settings,
    pub cleanup: CleanupSettings,
    pub dictionary: Vec<DictionaryEntry>,
    pub snippets: Vec<Snippet>,
    /// Daily totals for Insights; kept when old history is pruned (no text in it).
    pub usage: HashMap<Day, DayTotal>,
    pub calendar: Calendar,
    stamps: Vec<Option<SystemTime>>,
}

impl Store {
    pub fn load(paths: Paths) -> Self {
        let mut store = Self {
            settings: read_json(&paths.settings()),
            cleanup: read_json(&paths.cleanup()),
            dictionary: read_json(&paths.dictionary()),
            snippets: read_json(&paths.snippets()),
            usage: read_json(&paths.usage()),
            calendar: local_calendar(),
            stamps: Vec::new(),
            paths,
        };
        store.stamps = store.current_stamps();
        store
    }

    fn current_stamps(&self) -> Vec<Option<SystemTime>> {
        [self.paths.settings(), self.paths.cleanup(), self.paths.dictionary(), self.paths.snippets()]
            .iter()
            .map(|p| modified(p))
            .collect()
    }

    /// Re-reads the settings files if another process changed them. Returns whether anything did.
    pub fn reload_if_changed(&mut self) -> bool {
        let stamps = self.current_stamps();
        if stamps == self.stamps {
            return false;
        }
        self.reload();
        true
    }

    pub fn reload(&mut self) {
        self.settings = read_json(&self.paths.settings());
        self.cleanup = read_json(&self.paths.cleanup());
        self.dictionary = read_json(&self.paths.dictionary());
        self.snippets = read_json(&self.paths.snippets());
        self.stamps = self.current_stamps();
    }

    pub fn save_settings(&mut self) {
        if let Err(error) = write_json(&self.paths.settings(), &self.settings) {
            log::error!("Couldn't save settings: {error}");
        }
        self.stamps = self.current_stamps();
    }

    pub fn save_dictionary(&mut self) {
        if let Err(error) = write_json(&self.paths.dictionary(), &self.dictionary) {
            log::error!("Couldn't save the dictionary: {error}");
        }
        self.stamps = self.current_stamps();
    }

    /// Adds a dictation to the history and the daily totals.
    pub fn record(&mut self, record: &DictationRecord, category: AppCategory) {
        let line = match serde_json::to_string(record) {
            Ok(line) => line,
            Err(error) => return log::error!("Couldn't encode a history record: {error}"),
        };
        let appended = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.paths.history())
            .and_then(|mut file| writeln!(file, "{line}"));
        if let Err(error) = appended {
            log::error!("Couldn't save history: {error}");
        }
        let day = self.calendar.day(record.date);
        self.usage.entry(day).or_default().add(record, category);
        self.save_usage();
    }

    pub fn record_learned(&mut self, count: i64) {
        let day = self.calendar.day(SystemTime::now());
        let total = self.usage.entry(day).or_default();
        total.learned = (total.learned as i64 + count).max(0) as _;
        self.save_usage();
    }

    fn save_usage(&self) {
        if let Err(error) = write_json(&self.paths.usage(), &self.usage) {
            log::error!("Couldn't save usage totals: {error}");
        }
    }

    /// The most recent dictation's text, for "paste last transcript".
    pub fn last_transcript(&self) -> Option<String> {
        let file = fs::File::open(self.paths.history()).ok()?;
        BufReader::new(file)
            .lines()
            .map_while(Result::ok)
            .filter_map(|line| serde_json::from_str::<DictationRecord>(&line).ok())
            .last()
            .map(|record| record.text)
    }

    /// Drops history older than the retention setting (usage totals stay).
    pub fn prune_history(&self) {
        let Some(days) = self.settings.retention.days() else { return };
        let Ok(file) = fs::File::open(self.paths.history()) else { return };
        let cutoff = SystemTime::now() - Duration::from_secs(days * 86_400);
        let lines: Vec<String> = BufReader::new(file).lines().map_while(Result::ok).collect();
        let kept: Vec<&String> = lines
            .iter()
            .filter(|line| serde_json::from_str::<DictationRecord>(line).map(|r| r.date >= cutoff).unwrap_or(false))
            .collect();
        if kept.len() == lines.len() {
            return;
        }
        let tmp = self.paths.history().with_extension("tmp");
        let written = fs::File::create(&tmp).and_then(|mut file| {
            for line in &kept {
                writeln!(file, "{line}")?;
            }
            file.sync_all()
        });
        if written.is_ok() {
            let _ = fs::rename(&tmp, self.paths.history());
        }
    }
}

/// The local time zone, from the OS, so days and snippet times are local.
pub fn local_calendar() -> Calendar {
    Calendar::with_offset(local_offset_seconds)
}

#[cfg(unix)]
fn local_offset_seconds(instant: SystemTime) -> i32 {
    let seconds = kudu_core::calendar::unix_seconds(instant) as libc::time_t;
    // SAFETY: localtime_r fills the provided struct and doesn't keep the pointers.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&seconds, &mut tm).is_null() {
            return 0;
        }
        tm.tm_gmtoff as i32
    }
}

#[cfg(windows)]
fn local_offset_seconds(_instant: SystemTime) -> i32 {
    use windows::Win32::System::Time::{GetDynamicTimeZoneInformation, DYNAMIC_TIME_ZONE_INFORMATION};
    /// GetDynamicTimeZoneInformation's "daylight saving time is in effect".
    const TIME_ZONE_ID_DAYLIGHT: u32 = 2;
    let mut info = DYNAMIC_TIME_ZONE_INFORMATION::default();
    // SAFETY: plain out-parameter call.
    let id = unsafe { GetDynamicTimeZoneInformation(&mut info) };
    let bias = info.Bias + if id == TIME_ZONE_ID_DAYLIGHT { info.DaylightBias } else { info.StandardBias };
    -bias * 60
}

#[cfg(not(any(unix, windows)))]
fn local_offset_seconds(_instant: SystemTime) -> i32 {
    0
}
