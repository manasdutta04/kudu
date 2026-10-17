//! Recognising meeting apps by the program using the microphone, and deciding when a meeting
//! has started and ended. The same rules as the macOS app's `MeetingApps` / `MeetingDetector`.

use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeetingKind {
    Meeting,
    Browser,
}

/// An app that holds meetings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MeetingApp {
    pub name: String,
    pub kind: MeetingKind,
}

/// Program names (Windows executables without ".exe", Linux process names, Windows Store
/// package names), compared in lower case: a name matches when it starts with the key.
const PROGRAMS: &[(&str, &str, MeetingKind)] = &[
    ("zoom", "Zoom", MeetingKind::Meeting),
    ("cpthost", "Zoom", MeetingKind::Meeting),
    ("ms-teams", "Microsoft Teams", MeetingKind::Meeting),
    ("msteams", "Microsoft Teams", MeetingKind::Meeting),
    ("teams", "Microsoft Teams", MeetingKind::Meeting),
    ("webex", "Webex", MeetingKind::Meeting),
    ("ciscocollabhost", "Webex", MeetingKind::Meeting),
    ("ciscowebex", "Webex", MeetingKind::Meeting),
    ("atmgr", "Webex", MeetingKind::Meeting),
    ("slack", "Slack", MeetingKind::Meeting),
    ("discord", "Discord", MeetingKind::Meeting),
    ("skype", "Skype", MeetingKind::Meeting),
    ("gotomeeting", "GoTo Meeting", MeetingKind::Meeting),
    ("g2mcomm", "GoTo Meeting", MeetingKind::Meeting),
    ("goto", "GoTo Meeting", MeetingKind::Meeting),
    ("bluejeans", "BlueJeans", MeetingKind::Meeting),
    ("ringcentral", "RingCentral", MeetingKind::Meeting),
    ("whatsapp", "WhatsApp", MeetingKind::Meeting),
    ("5319275a.whatsappdesktop", "WhatsApp", MeetingKind::Meeting),
    ("telegram", "Telegram", MeetingKind::Meeting),
    ("signal", "Signal", MeetingKind::Meeting),
    ("chrome", "Google Chrome", MeetingKind::Browser),
    ("google-chrome", "Google Chrome", MeetingKind::Browser),
    ("chromium", "Chromium", MeetingKind::Browser),
    ("msedge", "Microsoft Edge", MeetingKind::Browser),
    ("microsoft-edge", "Microsoft Edge", MeetingKind::Browser),
    ("firefox", "Firefox", MeetingKind::Browser),
    ("brave", "Brave", MeetingKind::Browser),
    ("arc", "Arc", MeetingKind::Browser),
    ("opera", "Opera", MeetingKind::Browser),
    ("vivaldi", "Vivaldi", MeetingKind::Browser),
];

/// The meeting app a program name belongs to, if any: "Zoom.exe", "ms-teams", the path of a
/// program, or a Windows Store package name ("MSTeams_8wekyb3d8bbwe").
pub fn app_for_program(program: &str) -> Option<MeetingApp> {
    let name = program.rsplit(['\\', '/', '#']).next().unwrap_or(program).to_lowercase();
    let name = name.strip_suffix(".exe").unwrap_or(&name);
    PROGRAMS
        .iter()
        .find(|(key, _, _)| name.starts_with(key))
        .map(|(_, app, kind)| MeetingApp { name: (*app).to_string(), kind: *kind })
}

/// The app to name when several are using the microphone: a meeting app wins over a browser.
pub fn preferred(apps: &[MeetingApp]) -> Option<MeetingApp> {
    apps.iter().find(|a| a.kind == MeetingKind::Meeting).or(apps.first()).cloned()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeetingChange {
    Started(MeetingApp),
    Ended,
}

/// Decides when a meeting has started and ended from which meeting apps are using the
/// microphone, checked every couple of seconds. A start needs a few seconds of use (not a
/// quick mic test); an end needs the microphone to be released for a while.
#[derive(Clone, Debug)]
pub struct MeetingDetector {
    pub start_after: Duration,
    pub end_after: Duration,
    active: Option<MeetingApp>,
    candidate_since: Option<Instant>,
    last_seen: Option<Instant>,
}

impl Default for MeetingDetector {
    fn default() -> Self {
        Self {
            start_after: Duration::from_secs(3),
            end_after: Duration::from_secs(20),
            active: None,
            candidate_since: None,
            last_seen: None,
        }
    }
}

impl MeetingDetector {
    pub fn active(&self) -> Option<&MeetingApp> {
        self.active.as_ref()
    }

    pub fn observe(&mut self, apps: &[MeetingApp], now: Instant) -> Option<MeetingChange> {
        if let Some(app) = preferred(apps) {
            self.last_seen = Some(now);
            if self.active.is_some() {
                return None;
            }
            let since = *self.candidate_since.get_or_insert(now);
            if now.duration_since(since) >= self.start_after {
                self.active = Some(app.clone());
                self.candidate_since = None;
                return Some(MeetingChange::Started(app));
            }
            return None;
        }
        self.candidate_since = None;
        if self.active.is_some() && self.last_seen.is_some_and(|seen| now.duration_since(seen) >= self.end_after) {
            self.active = None;
            return Some(MeetingChange::Ended);
        }
        None
    }

    /// Forgets the current meeting, so the next one is noticed afresh.
    pub fn reset(&mut self) {
        *self = Self { start_after: self.start_after, end_after: self.end_after, ..Self::default() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zoom() -> MeetingApp {
        MeetingApp { name: "Zoom".into(), kind: MeetingKind::Meeting }
    }

    #[test]
    fn recognises_programs() {
        assert_eq!(app_for_program("Zoom.exe").unwrap().name, "Zoom");
        assert_eq!(app_for_program(r"C:#Program Files#Google#Chrome#Application#chrome.exe").unwrap().kind, MeetingKind::Browser);
        assert_eq!(app_for_program("MSTeams_8wekyb3d8bbwe").unwrap().name, "Microsoft Teams");
        assert_eq!(app_for_program("/usr/bin/firefox").unwrap().name, "Firefox");
        assert!(app_for_program("kudu.exe").is_none());
        assert!(app_for_program("parec").is_none());
    }

    #[test]
    fn starts_after_a_few_seconds_of_microphone_use() {
        let mut detector = MeetingDetector::default();
        let t0 = Instant::now();
        assert_eq!(detector.observe(&[zoom()], t0), None);
        assert_eq!(detector.observe(&[zoom()], t0 + Duration::from_secs(2)), None);
        assert_eq!(detector.observe(&[zoom()], t0 + Duration::from_secs(3)), Some(MeetingChange::Started(zoom())));
        assert_eq!(detector.observe(&[zoom()], t0 + Duration::from_secs(5)), None);
    }

    #[test]
    fn a_quick_microphone_test_is_not_a_meeting() {
        let mut detector = MeetingDetector::default();
        let t0 = Instant::now();
        detector.observe(&[zoom()], t0);
        detector.observe(&[], t0 + Duration::from_secs(2));
        assert_eq!(detector.observe(&[zoom()], t0 + Duration::from_secs(4)), None);
    }

    #[test]
    fn ends_only_after_the_microphone_is_released_for_a_while() {
        let mut detector = MeetingDetector::default();
        let t0 = Instant::now();
        let at = |s: u64| t0 + Duration::from_secs(s);
        detector.observe(&[zoom()], at(0));
        detector.observe(&[zoom()], at(3));
        assert_eq!(detector.observe(&[], at(10)), None);
        assert_eq!(detector.observe(&[zoom()], at(12)), None);
        assert_eq!(detector.observe(&[], at(30)), None);
        assert_eq!(detector.observe(&[], at(32)), Some(MeetingChange::Ended));
        assert!(detector.active().is_none());
    }

    #[test]
    fn prefers_a_meeting_app_over_a_browser() {
        let chrome = MeetingApp { name: "Google Chrome".into(), kind: MeetingKind::Browser };
        assert_eq!(preferred(&[chrome, zoom()]), Some(zoom()));
    }
}
