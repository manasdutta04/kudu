//! Meetings: the saved meeting, its transcript, and how they're shown and exported. The same
//! files and fields as the macOS app (`Meetings/<id>/meeting.json` and `transcript.jsonl`).

use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::calendar::iso8601;
use crate::id::new_id;
use crate::usage_stats::word_count;

/// Where a piece of meeting audio came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AudioSource {
    /// The user's microphone: "You".
    Mic,
    /// What the computer plays (the other people in the call): "Others".
    System,
}

impl AudioSource {
    pub fn speaker(self) -> &'static str {
        match self {
            AudioSource::Mic => "You",
            AudioSource::System => "Others",
        }
    }
}

/// One transcribed stretch of a meeting. Times are seconds from the start of the meeting.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeetingSegment {
    pub start: f64,
    pub end: f64,
    pub source: AudioSource,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MeetingStatus {
    /// Listening and transcribing.
    Recording,
    /// The meeting ended; notes are being written.
    Writing,
    Done,
}

/// What wrote a meeting's notes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotesGenerator {
    /// Apple Intelligence (Mac).
    #[serde(rename = "apple-intelligence")]
    AppleIntelligence,
    /// The pack's Kudu writer model (Windows and Linux).
    #[serde(rename = "kudu-writer")]
    KuduWriter,
    /// Kudu's built-in rules, when no language model is available.
    #[serde(rename = "extractive")]
    Extractive,
}

/// The written-up notes for a meeting.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingNotes {
    pub summary: String,
    #[serde(default)]
    pub key_points: Vec<String>,
    #[serde(default)]
    pub decisions: Vec<String>,
    #[serde(default)]
    pub actions: Vec<String>,
    pub generator: NotesGenerator,
}

impl MeetingNotes {
    /// Words across every section, for checking a model's notes aren't just the transcript.
    pub fn word_count(&self) -> usize {
        std::iter::once(&self.summary)
            .chain(&self.key_points)
            .chain(&self.decisions)
            .chain(&self.actions)
            .map(|text| word_count(text))
            .sum()
    }

    pub fn is_empty(&self) -> bool {
        self.summary.is_empty() && self.key_points.is_empty() && self.decisions.is_empty() && self.actions.is_empty()
    }
}

/// A recorded meeting.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meeting {
    pub id: String,
    pub title: String,
    #[serde(with = "iso8601")]
    pub started: SystemTime,
    #[serde(with = "optional_date", default, skip_serializing_if = "Option::is_none")]
    pub ended: Option<SystemTime>,
    /// The meeting app, when Kudu knows it ("Zoom", "Google Chrome").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    /// The audio that was recorded: the microphone, and the computer's sound when allowed.
    pub sources: Vec<AudioSource>,
    pub status: MeetingStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<MeetingNotes>,
    #[serde(default)]
    pub words: usize,
}

impl Meeting {
    pub fn new(app: Option<String>, sources: Vec<AudioSource>) -> Self {
        Self {
            id: new_id(),
            title: Self::default_title(app.as_deref()),
            started: SystemTime::now(),
            ended: None,
            app,
            sources,
            status: MeetingStatus::Recording,
            notes: None,
            words: 0,
        }
    }

    /// "Meeting in Zoom" or "Meeting", until notes give it a better title.
    pub fn default_title(app: Option<&str>) -> String {
        app.map_or_else(|| "Meeting".to_string(), |app| format!("Meeting in {app}"))
    }

    /// Length in seconds (up to `now` while it's still recording).
    pub fn duration(&self, now: SystemTime) -> f64 {
        self.ended.unwrap_or(now).duration_since(self.started).unwrap_or(Duration::ZERO).as_secs_f64()
    }

    pub fn heard_others(&self) -> bool {
        self.sources.contains(&AudioSource::System)
    }
}

mod optional_date {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &Option<SystemTime>, serializer: S) -> Result<S::Ok, S::Error> {
        match value {
            Some(instant) => iso8601::serialize(instant, serializer),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<SystemTime>, D::Error> {
        let text = Option::<String>::deserialize(deserializer)?;
        Ok(text.and_then(|text| iso8601::parse(&text)))
    }
}

/// "4:05" or "1:02:09".
pub fn clock(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let (hours, minutes, secs) = (total / 3600, total % 3600 / 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}")
    }
}

/// "34 min", "1 h 5 min", "under a minute".
pub fn length(seconds: f64) -> String {
    if seconds < 60.0 {
        return "under a minute".into();
    }
    let minutes = (seconds / 60.0).round() as u64;
    if minutes < 60 {
        return format!("{minutes} min");
    }
    match minutes % 60 {
        0 => format!("{} h", minutes / 60),
        rest => format!("{} h {rest} min", minutes / 60),
    }
}

/// Consecutive segments from the same speaker joined into one turn, in time order.
pub fn turns(segments: &[MeetingSegment]) -> Vec<MeetingSegment> {
    let mut sorted = segments.to_vec();
    sorted.sort_by(|a, b| a.start.total_cmp(&b.start));
    let mut turns: Vec<MeetingSegment> = Vec::new();
    for segment in sorted {
        let text = segment.text.trim();
        if text.is_empty() {
            continue;
        }
        match turns.last_mut() {
            Some(last) if last.source == segment.source && segment.start - last.end < 30.0 => {
                last.text.push(' ');
                last.text.push_str(text);
                last.end = segment.end;
            }
            _ => turns.push(MeetingSegment { text: text.to_string(), ..segment }),
        }
    }
    turns
}

/// The transcript as plain text for a language model: "You: …" and "Others: …" lines, or just
/// the text when only the microphone was recorded.
pub fn transcript_text(segments: &[MeetingSegment], labelled: bool) -> String {
    turns(segments)
        .iter()
        .map(|turn| if labelled { format!("{}: {}", turn.source.speaker(), turn.text) } else { turn.text.clone() })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The meeting as a Markdown document: notes, then the transcript.
pub fn markdown(meeting: &Meeting, segments: &[MeetingSegment], date_text: &str, now: SystemTime) -> String {
    let mut lines = vec![format!("# {}", meeting.title), String::new()];
    let mut meta = vec![date_text.to_string(), length(meeting.duration(now))];
    if let Some(app) = &meeting.app {
        meta.push(app.clone());
    }
    lines.push(meta.join(" · "));
    lines.push(String::new());
    if let Some(notes) = &meeting.notes {
        if !notes.summary.is_empty() {
            lines.extend(["## Summary".into(), String::new(), notes.summary.clone(), String::new()]);
        }
        for (title, items, prefix) in [
            ("Key points", &notes.key_points, "- "),
            ("Decisions", &notes.decisions, "- "),
            ("Action items", &notes.actions, "- [ ] "),
        ] {
            if items.is_empty() {
                continue;
            }
            lines.push(format!("## {title}"));
            lines.push(String::new());
            lines.extend(items.iter().map(|item| format!("{prefix}{item}")));
            lines.push(String::new());
        }
    }
    let labelled = meeting.heard_others();
    lines.extend(["## Transcript".into(), String::new()]);
    for turn in turns(segments) {
        let stamp = clock(turn.start);
        lines.push(if labelled {
            format!("**{}** ({stamp}): {}", turn.source.speaker(), turn.text)
        } else {
            format!("({stamp}) {}", turn.text)
        });
        lines.push(String::new());
    }
    format!("{}\n", lines.join("\n").trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f64, end: f64, source: AudioSource, text: &str) -> MeetingSegment {
        MeetingSegment { start, end, source, text: text.into() }
    }

    #[test]
    fn formats_clock_and_length() {
        assert_eq!(clock(65.0), "1:05");
        assert_eq!(clock(3_729.0), "1:02:09");
        assert_eq!(length(30.0), "under a minute");
        assert_eq!(length(34.0 * 60.0), "34 min");
        assert_eq!(length(65.0 * 60.0), "1 h 5 min");
    }

    #[test]
    fn joins_turns_from_the_same_speaker() {
        let segments = [
            seg(0.0, 5.0, AudioSource::System, "Hi there."),
            seg(5.0, 9.0, AudioSource::System, "How was the trip?"),
            seg(9.0, 12.0, AudioSource::Mic, "Great, thanks."),
        ];
        assert_eq!(transcript_text(&segments, true), "Others: Hi there. How was the trip?\nYou: Great, thanks.");
        assert_eq!(transcript_text(&segments, false), "Hi there. How was the trip?\nGreat, thanks.");
    }

    #[test]
    fn markdown_has_notes_and_transcript() {
        let mut meeting = Meeting::new(Some("Zoom".into()), vec![AudioSource::Mic, AudioSource::System]);
        meeting.title = "Weekly sync".into();
        meeting.ended = Some(meeting.started + Duration::from_secs(600));
        meeting.notes = Some(MeetingNotes {
            summary: "Short.".into(),
            key_points: vec![],
            decisions: vec![],
            actions: vec!["Send the deck".into()],
            generator: NotesGenerator::KuduWriter,
        });
        let text = markdown(&meeting, &[seg(3.0, 6.0, AudioSource::Mic, "Hello.")], "Today", SystemTime::now());
        assert!(text.contains("# Weekly sync"));
        assert!(text.contains("Today · 10 min · Zoom"));
        assert!(text.contains("- [ ] Send the deck"));
        assert!(text.contains("**You** (0:03): Hello."));
    }

    #[test]
    fn reads_the_mac_apps_files() {
        let json = r#"{"app":"Zoom","ended":"2026-10-17T10:30:00Z","id":"A","notes":{"actions":[],"decisions":[],"generator":"apple-intelligence","keyPoints":["One"],"summary":"S"},"sources":["mic","system"],"started":"2026-10-17T10:00:00Z","status":"done","title":"Sync","words":120}"#;
        let meeting: Meeting = serde_json::from_str(json).unwrap();
        assert_eq!(meeting.duration(SystemTime::now()), 1800.0);
        assert!(meeting.heard_others());
        assert_eq!(meeting.notes.as_ref().unwrap().generator, NotesGenerator::AppleIntelligence);
        let back: Meeting = serde_json::from_str(&serde_json::to_string(&meeting).unwrap()).unwrap();
        assert_eq!(back, meeting);
        let segment: MeetingSegment = serde_json::from_str(r#"{"end":6,"source":"mic","start":3,"text":"Hi"}"#).unwrap();
        assert_eq!(segment.source, AudioSource::Mic);
    }
}
