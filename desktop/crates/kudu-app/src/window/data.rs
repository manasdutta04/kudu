//! The files the window shows and edits, loaded when they change (never per frame).

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use kudu_core::calendar::unix_seconds;
use kudu_core::shortcut::Shortcuts;
use kudu_core::{Calendar, CleanupSettings, DayTotal, Day, DictationRecord, DictionaryEntry, Insights, Snippet, UsageStats};
use kudu_engine::{SpeechModel, WriterModel};

use crate::paths::{modified, read_json, write_json, Paths};
use crate::settings::Settings;

/// Small window-only preferences (dismissed intro cards).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WindowPrefs {
    pub dismissed_dictionary_intro: bool,
    pub dismissed_snippets_intro: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum File {
    Settings,
    Cleanup,
    Dictionary,
    Snippets,
    History,
    Usage,
}

const FILES: [File; 6] = [File::Settings, File::Cleanup, File::Dictionary, File::Snippets, File::History, File::Usage];

pub struct Data {
    pub paths: Paths,
    pub settings: Settings,
    pub cleanup: CleanupSettings,
    pub dictionary: Vec<DictionaryEntry>,
    pub snippets: Vec<Snippet>,
    /// Newest first.
    pub history: Vec<DictationRecord>,
    pub usage: HashMap<Day, DayTotal>,
    pub stats: UsageStats,
    pub insights: Insights,
    pub prefs: WindowPrefs,
    pub calendar: Calendar,
    /// Which models are on disk (checked when files or the status change).
    pub speech_installed: Vec<SpeechModel>,
    pub writer_installed: Vec<WriterModel>,
    /// Bumped whenever the history changes, for cached views of it.
    pub history_version: u64,
    stamps: [Option<SystemTime>; 6],
}

impl Data {
    pub fn load(paths: Paths) -> Self {
        let calendar = local_calendar();
        let now = SystemTime::now();
        let mut data = Self {
            prefs: read_json(&paths.config.join("window.json")),
            paths,
            settings: Settings::default(),
            cleanup: CleanupSettings::default(),
            dictionary: Vec::new(),
            snippets: Vec::new(),
            history: Vec::new(),
            usage: HashMap::new(),
            stats: UsageStats::EMPTY,
            insights: Insights::compute(&HashMap::new(), now, Insights::DEFAULT_WEEKS, &calendar),
            calendar,
            speech_installed: Vec::new(),
            writer_installed: Vec::new(),
            history_version: 0,
            stamps: [None; 6],
        };
        for file in FILES {
            data.read(file);
        }
        data.check_models();
        data
    }

    fn path(&self, file: File) -> PathBuf {
        match file {
            File::Settings => self.paths.settings(),
            File::Cleanup => self.paths.cleanup(),
            File::Dictionary => self.paths.dictionary(),
            File::Snippets => self.paths.snippets(),
            File::History => self.paths.history(),
            File::Usage => self.paths.usage(),
        }
    }

    /// The files to watch for changes made by the background process.
    pub fn watched(&self) -> Vec<PathBuf> {
        FILES.iter().map(|f| self.path(*f)).collect()
    }

    fn read(&mut self, file: File) {
        let path = self.path(file);
        self.stamps[file as usize] = modified(&path);
        match file {
            File::Settings => self.settings = read_json(&path),
            File::Cleanup => self.cleanup = read_json(&path),
            File::Dictionary => self.dictionary = read_json(&path),
            File::Snippets => self.snippets = read_json(&path),
            File::History => {
                self.history = read_history(&path);
                self.history_version += 1;
                self.stats = UsageStats::compute(&self.history, SystemTime::now(), &self.calendar);
            }
            File::Usage => {
                self.usage = read_json(&path);
                self.insights = Insights::compute(&self.usage, SystemTime::now(), Insights::DEFAULT_WEEKS, &self.calendar);
            }
        }
    }

    /// Re-reads the files whose modification time changed. Returns whether any did.
    pub fn reload_changed(&mut self) -> bool {
        let mut changed = false;
        for file in FILES {
            if modified(&self.path(file)) != self.stamps[file as usize] {
                self.read(file);
                changed = true;
            }
        }
        if changed {
            self.check_models();
        }
        changed
    }

    /// Re-reads history, usage and the dictionary (after `DataChanged`).
    pub fn reload_data(&mut self) {
        for file in [File::History, File::Usage, File::Dictionary] {
            self.read(file);
        }
    }

    pub fn check_models(&mut self) {
        let models = &self.paths.models;
        self.speech_installed = SpeechModel::ALL.into_iter().filter(|m| m.is_installed(models)).collect();
        self.writer_installed = WriterModel::ALL.into_iter().filter(|m| m.is_installed(models)).collect();
    }

    /// The shortcuts in effect (the platform defaults until the user changes one).
    pub fn shortcuts(&self) -> Shortcuts {
        self.settings.shortcuts.clone().unwrap_or_else(default_shortcuts)
    }

    // Writes re-read the file first, so a change the background process just made (a recorded
    // shortcut, a learned word) isn't overwritten with an older copy.

    pub fn update_settings(&mut self, change: impl FnOnce(&mut Settings)) {
        let path = self.paths.settings();
        let mut settings: Settings = read_json(&path);
        change(&mut settings);
        self.save(File::Settings, &settings);
        self.settings = settings;
    }

    pub fn update_cleanup(&mut self, change: impl FnOnce(&mut CleanupSettings)) {
        let mut cleanup: CleanupSettings = read_json(&self.paths.cleanup());
        change(&mut cleanup);
        self.save(File::Cleanup, &cleanup);
        self.cleanup = cleanup;
    }

    pub fn update_dictionary(&mut self, change: impl FnOnce(&mut Vec<DictionaryEntry>)) {
        let mut dictionary: Vec<DictionaryEntry> = read_json(&self.paths.dictionary());
        change(&mut dictionary);
        self.save(File::Dictionary, &dictionary);
        self.dictionary = dictionary;
    }

    pub fn update_snippets(&mut self, change: impl FnOnce(&mut Vec<Snippet>)) {
        let mut snippets: Vec<Snippet> = read_json(&self.paths.snippets());
        change(&mut snippets);
        self.save(File::Snippets, &snippets);
        self.snippets = snippets;
    }

    fn save<T: serde::Serialize>(&mut self, file: File, value: &T) {
        let path = self.path(file);
        if let Err(error) = write_json(&path, value) {
            log::error!("Couldn't save {}: {error:#}", path.display());
        }
        self.stamps[file as usize] = modified(&path);
    }

    pub fn save_prefs(&self) {
        if let Err(error) = write_json(&self.paths.config.join("window.json"), &self.prefs) {
            log::warn!("Couldn't save the window preferences: {error:#}");
        }
    }

    /// Rewrites the history without the records `keep` rejects.
    pub fn rewrite_history(&mut self, keep: impl Fn(&DictationRecord) -> bool) {
        let path = self.paths.history();
        let records = read_history(&path);
        let mut out = String::new();
        // The file is oldest first; `read_history` returns newest first.
        for record in records.iter().rev().filter(|r| keep(r)) {
            if let Ok(line) = serde_json::to_string(record) {
                out.push_str(&line);
                out.push('\n');
            }
        }
        let tmp = path.with_extension("tmp");
        if let Err(error) = fs::write(&tmp, out).and_then(|()| fs::rename(&tmp, &path)) {
            log::error!("Couldn't rewrite the history: {error}");
        }
        self.read(File::History);
    }
}

fn read_history(path: &std::path::Path) -> Vec<DictationRecord> {
    let Ok(text) = fs::read_to_string(path) else { return Vec::new() };
    let mut records: Vec<DictationRecord> =
        text.lines().filter(|l| !l.trim().is_empty()).filter_map(|l| serde_json::from_str(l).ok()).collect();
    records.sort_by_key(|r| std::cmp::Reverse(r.date));
    records
}

/// Ctrl + Win to dictate (with the platform's key code for V in the paste shortcut).
pub fn default_shortcuts() -> Shortcuts {
    // Windows virtual-key code / Linux evdev code for V.
    Shortcuts::default_pc(if cfg!(windows) { 0x56 } else { 47 })
}

// MARK: - Local time

/// The calendar in this computer's time zone.
pub fn local_calendar() -> Calendar {
    Calendar::with_offset(local_offset)
}

/// Seconds east of UTC at `instant` in the local time zone.
#[cfg(unix)]
pub fn local_offset(instant: SystemTime) -> i32 {
    #[repr(C)]
    struct Tm {
        tm_sec: i32,
        tm_min: i32,
        tm_hour: i32,
        tm_mday: i32,
        tm_mon: i32,
        tm_year: i32,
        tm_wday: i32,
        tm_yday: i32,
        tm_isdst: i32,
        tm_gmtoff: std::ffi::c_long,
        tm_zone: *const std::ffi::c_char,
    }
    unsafe extern "C" {
        fn localtime_r(time: *const i64, result: *mut Tm) -> *mut Tm;
        fn tzset();
    }
    static TZSET: std::sync::Once = std::sync::Once::new();
    // SAFETY: `tzset` takes no arguments; `localtime_r` writes only into `tm`, which is a
    // correctly laid out `struct tm` (glibc, musl and macOS share this layout on 64-bit).
    unsafe {
        TZSET.call_once(|| tzset());
        let seconds = unix_seconds(instant);
        let mut tm: Tm = std::mem::zeroed();
        if localtime_r(&seconds, &mut tm).is_null() {
            return 0;
        }
        tm.tm_gmtoff as i32
    }
}

#[cfg(windows)]
pub fn local_offset(instant: SystemTime) -> i32 {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::Storage::FileSystem::FileTimeToLocalFileTime;
    const EPOCH_DIFFERENCE: i64 = 11_644_473_600;
    let ticks = (unix_seconds(instant) + EPOCH_DIFFERENCE) * 10_000_000;
    let utc = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
    let mut local = FILETIME::default();
    // SAFETY: both pointers refer to valid FILETIME values on the stack.
    if unsafe { FileTimeToLocalFileTime(&utc, &mut local) }.is_err() {
        return 0;
    }
    let local_ticks = (i64::from(local.dwHighDateTime) << 32) | i64::from(local.dwLowDateTime);
    ((local_ticks - ticks) / 10_000_000) as i32
}

#[cfg(not(any(unix, windows)))]
pub fn local_offset(_instant: SystemTime) -> i32 {
    0
}

const MONTHS: [&str; 12] =
    ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

pub fn month_name(month: u32) -> &'static str {
    MONTHS[(month.clamp(1, 12) - 1) as usize]
}

pub fn month_short(month: u32) -> &'static str {
    &month_name(month)[..3]
}

const WEEKDAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/// "Wednesday, Oct 14".
pub fn day_long(day: Day) -> String {
    let (_, month, date) = day.ymd();
    format!("{}, {} {date}", WEEKDAYS[(day.weekday() as usize).clamp(1, 7) - 1], month_short(month))
}

/// "TODAY", "YESTERDAY" or "OCTOBER 12, 2026".
pub fn day_title(day: Day, today: Day) -> String {
    if day == today {
        return "TODAY".into();
    }
    if day == today.add_days(-1) {
        return "YESTERDAY".into();
    }
    let (year, month, date) = day.ymd();
    format!("{} {date}, {year}", month_name(month).to_uppercase())
}

/// Whether times read as "3:42 PM" here (US English and the like) rather than "15:42".
pub fn twelve_hour_clock() -> bool {
    static TWELVE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *TWELVE.get_or_init(|| {
        let locale = ["LC_ALL", "LC_TIME", "LANG"]
            .iter()
            .find_map(|key| std::env::var(key).ok().filter(|v| !v.is_empty()))
            .unwrap_or_default();
        if cfg!(windows) || locale.is_empty() || locale == "C" || locale.starts_with("C.") {
            return true;
        }
        ["en_US", "en_CA", "en_AU", "en_NZ", "en_IN", "en_PH", "es_US", "hi_IN"].iter().any(|l| locale.starts_with(l))
    })
}

/// "3:42 PM" or "15:42".
pub fn time_of_day(seconds: u32) -> String {
    let (hour, minute) = (seconds / 3600, seconds / 60 % 60);
    if twelve_hour_clock() {
        let suffix = if hour < 12 { "AM" } else { "PM" };
        let hour12 = match hour % 12 {
            0 => 12,
            h => h,
        };
        format!("{hour12}:{minute:02} {suffix}")
    } else {
        format!("{hour:02}:{minute:02}")
    }
}

/// 1234 → "1,234".
pub fn grouped(number: usize) -> String {
    let digits = number.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// 1234 → "1.2K", 12345 → "12K".
pub fn compact(number: usize) -> String {
    match number {
        1_000_000.. => format!("{:.1}M", number as f64 / 1_000_000.0),
        10_000.. => format!("{:.0}K", number as f64 / 1_000.0),
        1_000.. => format!("{:.1}K", number as f64 / 1_000.0),
        _ => number.to_string(),
    }
}

/// The user's first name, from the OS account (full name when the system has one).
pub fn first_name() -> String {
    let user = std::env::var(if cfg!(windows) { "USERNAME" } else { "USER" }).unwrap_or_default();
    #[cfg(target_os = "linux")]
    {
        // The comment (GECOS) field of /etc/passwd holds the full name: "Manas Dutta,,,".
        if let Ok(passwd) = fs::read_to_string("/etc/passwd") {
            let full = passwd.lines().find_map(|line| {
                let fields: Vec<&str> = line.split(':').collect();
                (fields.len() > 4 && fields[0] == user).then(|| fields[4].split(',').next().unwrap_or("").trim().to_string())
            });
            if let Some(first) = full.as_deref().and_then(|f| f.split_whitespace().next()) {
                return first.to_string();
            }
        }
    }
    let name = user.split(['.', '_', ' ', '-']).next().unwrap_or("").to_string();
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => "there".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_numbers_and_days() {
        assert_eq!(grouped(1_234_567), "1,234,567");
        assert_eq!(grouped(12), "12");
        assert_eq!(compact(1_234), "1.2K");
        assert_eq!(compact(12_345), "12K");
        let today = Day::from_ymd(2026, 10, 14).unwrap();
        assert_eq!(day_title(today, today), "TODAY");
        assert_eq!(day_title(today.add_days(-1), today), "YESTERDAY");
        assert_eq!(day_title(today.add_days(-2), today), "OCTOBER 12, 2026");
        assert_eq!(day_long(today), "Wednesday, Oct 14");
    }
}
