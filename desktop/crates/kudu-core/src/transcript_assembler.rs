//! Puts the two transcripts of a meeting (the microphone and the computer's sound) together,
//! and drops what the microphone heard from the speakers. Without headphones the microphone
//! picks up the other people too, so the same words would otherwise appear twice, once as
//! "You". The same rules as the macOS app's `TranscriptAssembler`.

use std::collections::HashSet;

use crate::meeting::{AudioSource, MeetingSegment};

/// A microphone segment is held back until the computer's sound has been transcribed past it
/// (plus `window` seconds), so it can be compared with what the others said at that time.
#[derive(Clone, Debug)]
pub struct TranscriptAssembler {
    /// How far apart in time an echo and its source can be, in seconds.
    pub window: f64,
    /// The share of a microphone segment's words that must match to call it an echo.
    pub threshold: f64,
    /// False when only the microphone is recorded: segments are released at once.
    pub has_system_audio: bool,
    pending: Vec<MeetingSegment>,
    recent_system: Vec<MeetingSegment>,
    system_heard_until: f64,
}

impl TranscriptAssembler {
    pub fn new(has_system_audio: bool) -> Self {
        Self { window: 4.0, threshold: 0.6, has_system_audio, pending: Vec::new(), recent_system: Vec::new(), system_heard_until: 0.0 }
    }

    /// Adds a transcribed segment; returns the segments that are ready to save, in time order.
    pub fn add(&mut self, segment: MeetingSegment) -> Vec<MeetingSegment> {
        if !self.has_system_audio {
            return vec![segment];
        }
        match segment.source {
            AudioSource::System => {
                self.recent_system.push(segment.clone());
                self.system_heard_until = self.system_heard_until.max(segment.end);
                let mut ready = vec![segment];
                ready.extend(self.release());
                ready.sort_by(|a, b| a.start.total_cmp(&b.start));
                ready
            }
            AudioSource::Mic => {
                self.pending.push(segment);
                self.release()
            }
        }
    }

    /// The computer's sound has been listened to up to `time`, even if nothing was said, so
    /// microphone segments before it can be released.
    pub fn system_heard(&mut self, until: f64) -> Vec<MeetingSegment> {
        if !self.has_system_audio {
            return Vec::new();
        }
        self.system_heard_until = self.system_heard_until.max(until);
        self.release()
    }

    /// The meeting ended: everything still held back.
    pub fn finish(&mut self) -> Vec<MeetingSegment> {
        let pending = std::mem::take(&mut self.pending);
        pending.into_iter().filter(|segment| !self.is_echo(segment)).collect()
    }

    fn release(&mut self) -> Vec<MeetingSegment> {
        let until = self.system_heard_until;
        let window = self.window;
        let (due, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending).into_iter().partition(|s| s.end + window <= until);
        self.pending = waiting;
        if due.is_empty() {
            return Vec::new();
        }
        let ready = due.into_iter().filter(|segment| !self.is_echo(segment)).collect();
        let oldest = self.pending.iter().map(|s| s.start).fold(until, f64::min) - window * 2.0;
        self.recent_system.retain(|s| s.end >= oldest);
        ready
    }

    fn is_echo(&self, mic: &MeetingSegment) -> bool {
        let heard: HashSet<String> = self
            .recent_system
            .iter()
            .filter(|s| s.end >= mic.start - self.window && s.start <= mic.end + self.window)
            .flat_map(|s| words(&s.text))
            .collect();
        overlap(&words(&mic.text), &heard) >= self.threshold
    }
}

/// The share of `words` found in `heard`. Very short segments must match completely.
fn overlap(words: &[String], heard: &HashSet<String>) -> f64 {
    if words.is_empty() || heard.is_empty() {
        return 0.0;
    }
    let found = words.iter().filter(|w| heard.contains(*w)).count();
    if words.len() < 3 {
        return if found == words.len() { 1.0 } else { 0.0 };
    }
    found as f64 / words.len() as f64
}

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f64, end: f64, source: AudioSource, text: &str) -> MeetingSegment {
        MeetingSegment { start, end, source, text: text.into() }
    }

    #[test]
    fn microphone_only_is_released_at_once() {
        let mut assembler = TranscriptAssembler::new(false);
        assert_eq!(assembler.add(seg(0.0, 5.0, AudioSource::Mic, "Hello everyone")).len(), 1);
    }

    #[test]
    fn drops_the_speakers_heard_by_the_microphone() {
        let mut assembler = TranscriptAssembler::new(true);
        assert!(assembler.add(seg(10.0, 16.0, AudioSource::Mic, "so the launch moves to next Tuesday because of the review")).is_empty());
        let released = assembler.add(seg(9.6, 15.8, AudioSource::System, "So the launch moves to next Tuesday, because of the review."));
        assert_eq!(released.iter().map(|s| s.source).collect::<Vec<_>>(), vec![AudioSource::System]);
        assert!(assembler.system_heard(30.0).is_empty());
    }

    #[test]
    fn keeps_what_the_user_said() {
        let mut assembler = TranscriptAssembler::new(true);
        assembler.add(seg(0.0, 6.0, AudioSource::System, "Can you send the deck before the call?"));
        assert!(assembler.add(seg(6.5, 9.0, AudioSource::Mic, "Sure, I'll send it this afternoon.")).is_empty());
        let released = assembler.system_heard(14.0);
        assert_eq!(released.len(), 1);
        assert_eq!(released[0].text, "Sure, I'll send it this afternoon.");
    }

    #[test]
    fn holds_the_microphone_until_the_others_are_transcribed_past_it() {
        let mut assembler = TranscriptAssembler::new(true);
        assert!(assembler.add(seg(0.0, 5.0, AudioSource::Mic, "Let's start with the budget")).is_empty());
        assert!(assembler.system_heard(8.0).is_empty());
        assert_eq!(assembler.system_heard(9.0).len(), 1);
    }

    #[test]
    fn finish_releases_the_rest() {
        let mut assembler = TranscriptAssembler::new(true);
        assembler.add(seg(0.0, 5.0, AudioSource::Mic, "Thanks everyone, talk soon"));
        assert_eq!(assembler.finish().len(), 1);
    }
}
