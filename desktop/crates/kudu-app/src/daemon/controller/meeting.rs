//! Meeting notes: records the microphone and the computer's sound, has the worker transcribe
//! them piece by piece while the meeting goes on (between dictations), saves the transcript
//! as it grows, and has the writer model write the notes when the meeting ends. The audio
//! itself is never saved.

use std::collections::VecDeque;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use kudu_core::meeting::{AudioSource, Meeting, MeetingNotes, MeetingSegment, MeetingStatus, NotesGenerator};
use kudu_core::meeting_apps::{MeetingChange, MeetingDetector};
use kudu_core::transcript_assembler::TranscriptAssembler;
use kudu_core::{meeting, meeting_notes, word_count, DictionaryStage, TextContext, TextPipeline};
use kudu_engine::meeting_audio::MeetingCapture;

use super::Controller;
use crate::daemon::worker::MeetingJob;
use crate::ipc::{Event, MeetingState, ModelStatus};
use crate::meetings;
use crate::overlay::OverlayState;
use crate::settings::MeetingAutoStart;

/// Long meetings stop by themselves after this long.
const MAXIMUM_LENGTH: f64 = 4.0 * 60.0 * 60.0;
/// Shorter recordings aren't kept.
const MINIMUM_LENGTH: f64 = 10.0;
/// How often to check which apps use the microphone.
const DETECTION_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Default)]
pub(super) struct MeetingNotesState {
    state: State,
    detector: MeetingDetector,
    /// Whether this computer could tell which apps use the microphone, at the last check.
    pub detection_available: bool,
    last_check: Option<Instant>,
    /// Saved meetings waiting for their notes (interrupted by a quit, or "write again").
    queue: VecDeque<String>,
    recovered: bool,
}

#[derive(Default)]
enum State {
    #[default]
    Idle,
    Recording(Box<Recording>),
    Writing(Box<Writing>),
}

struct Recording {
    meeting: Meeting,
    capture: MeetingCapture,
    assembler: TranscriptAssembler,
    from_detection: bool,
    /// Pieces sent to the worker and not back yet (all sources, and the computer's sound).
    in_flight: usize,
    system_in_flight: usize,
    stopping: bool,
}

struct Writing {
    meeting: Meeting,
    segments: Vec<MeetingSegment>,
    /// The transcript in parts (one part: the notes are written from the whole transcript).
    parts: Vec<String>,
    summaries: Vec<String>,
    words: usize,
}

impl Writing {
    fn total_steps(&self) -> usize {
        if self.parts.len() > 1 { self.parts.len() + 1 } else { 1 }
    }
}

impl Controller {
    fn data(&self) -> std::path::PathBuf {
        self.store.paths.data.clone()
    }

    pub fn meeting_state(&self) -> MeetingState {
        match &self.meetings.state {
            State::Idle => MeetingState::Idle,
            State::Recording(recording) => MeetingState::Recording {
                id: recording.meeting.id.clone(),
                started: recording.meeting.started.duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64()),
                app: recording.meeting.app.clone(),
                system_audio: recording.meeting.heard_others(),
            },
            State::Writing(writing) => MeetingState::Writing {
                id: writing.meeting.id.clone(),
                done: writing.summaries.len(),
                total: writing.total_steps(),
            },
        }
    }

    pub fn meeting_is_recording(&self) -> bool {
        matches!(self.meetings.state, State::Recording(_))
    }

    /// Silences the meeting's microphone while the user dictates into another app.
    pub(super) fn set_meeting_mic_muted(&self, muted: bool) {
        if let State::Recording(recording) = &self.meetings.state {
            recording.capture.set_mic_muted(muted);
        }
    }

    /// The shortcut, the tray and the Meetings page: start, or stop if recording.
    pub fn toggle_meeting(&mut self) {
        if !self.store.settings.meetings.enabled {
            self.flash("Turn on meeting notes in Kudu → Meetings");
            return;
        }
        if self.meeting_is_recording() {
            self.stop_meeting();
        } else {
            let app = self.meetings.detector.active().map(|app| app.name.clone());
            self.start_meeting(app, false);
        }
    }

    pub fn start_meeting(&mut self, app: Option<String>, from_detection: bool) {
        if !self.store.settings.meetings.enabled || !matches!(self.meetings.state, State::Idle) {
            if matches!(self.meetings.state, State::Writing(_)) {
                self.flash("Still writing the last meeting's notes");
            }
            return;
        }
        if self.speech != ModelStatus::Ready {
            self.flash("Download a speech model first");
            return;
        }
        let settings = &self.store.settings;
        let capture = match MeetingCapture::start(settings.microphone.as_deref(), settings.meetings.system_audio) {
            Ok(capture) => capture,
            Err(error) => {
                log::error!("Couldn't start meeting notes: {error:#}");
                self.flash("Couldn't open the microphone");
                return;
            }
        };
        if self.is_recording() {
            capture.set_mic_muted(true);
        }
        let sources = if capture.has_system_audio() { vec![AudioSource::Mic, AudioSource::System] } else { vec![AudioSource::Mic] };
        let meeting = Meeting::new(app.clone(), sources);
        meetings::save(&self.data(), &meeting);
        log::info!("Meeting notes started (app: {}, others' audio: {})", app.as_deref().unwrap_or("none"), capture.has_system_audio());
        self.meetings.state = State::Recording(Box::new(Recording {
            assembler: TranscriptAssembler::new(capture.has_system_audio()),
            meeting,
            capture,
            from_detection,
            in_flight: 0,
            system_in_flight: 0,
            stopping: false,
        }));
        if self.store.settings.sounds {
            kudu_engine::sounds::play(kudu_engine::sounds::Cue::Start);
        }
        self.flash(&app.map_or_else(|| "Taking meeting notes".into(), |app| format!("Taking notes · {app}")));
        self.refresh_tray();
        let _ = self.window.send(Event::DataChanged);
    }

    pub fn stop_meeting(&mut self) {
        let State::Recording(recording) = &mut self.meetings.state else { return };
        if recording.stopping {
            return;
        }
        recording.stopping = true;
        recording.capture.stop();
        let pieces = recording.capture.take_pieces(true);
        self.send_pieces(pieces);
        if self.store.settings.sounds {
            kudu_engine::sounds::play(kudu_engine::sounds::Cue::Stop);
        }
        self.flash("Writing meeting notes…");
        self.finish_recording_if_done();
    }

    /// Called a few times a second: hands finished pieces to the worker, watches for meetings
    /// starting and ending, and finishes meetings interrupted by a quit.
    pub(super) fn meeting_tick(&mut self) {
        if !self.meetings.recovered && self.speech != ModelStatus::Loading {
            self.meetings.recovered = true;
            self.recover_interrupted();
        }
        if let State::Recording(recording) = &mut self.meetings.state {
            if !recording.stopping {
                let pieces = recording.capture.take_pieces(false);
                let too_long = recording.meeting.duration(SystemTime::now()) >= MAXIMUM_LENGTH;
                self.send_pieces(pieces);
                self.release_quiet_system();
                if too_long {
                    log::info!("Meeting reached the maximum length; stopping");
                    self.stop_meeting();
                }
            }
        }
        self.check_for_meetings();
    }

    fn send_pieces(&mut self, pieces: Vec<kudu_engine::meeting_audio::Piece>) {
        let State::Recording(recording) = &mut self.meetings.state else { return };
        for piece in pieces.into_iter().filter(|piece| piece.has_speech()) {
            recording.in_flight += 1;
            if piece.source == AudioSource::System {
                recording.system_in_flight += 1;
            }
            let end = piece.end();
            let _ = self.meeting_jobs.send(MeetingJob::Transcribe {
                id: recording.meeting.id.clone(),
                source: piece.source,
                start: piece.start,
                end,
                samples: piece.samples,
            });
        }
    }

    /// The computer's sound was quiet (nothing waiting to transcribe): microphone segments
    /// before that point can be saved.
    fn release_quiet_system(&mut self) {
        let State::Recording(recording) = &mut self.meetings.state else { return };
        if recording.system_in_flight > 0 {
            return;
        }
        if let Some(until) = recording.capture.system_quiet_until() {
            let ready = recording.assembler.system_heard(until);
            self.save_segments(ready);
        }
    }

    /// A transcribed piece came back from the worker.
    pub(super) fn meeting_text(&mut self, id: String, source: AudioSource, start: f64, end: f64, text: String) {
        let dictionary = DictionaryStage::new(&self.store.dictionary);
        let State::Recording(recording) = &mut self.meetings.state else { return };
        if recording.meeting.id != id {
            return;
        }
        recording.in_flight = recording.in_flight.saturating_sub(1);
        if source == AudioSource::System {
            recording.system_in_flight = recording.system_in_flight.saturating_sub(1);
        }
        let context = TextContext::default();
        let text = TextPipeline::basic().process(&kudu_core::TextStage::apply(&dictionary, &text, &context), &context);
        let mut ready = Vec::new();
        if word_count(&text) > 0 {
            ready.extend(recording.assembler.add(MeetingSegment { start, end, source, text }));
        }
        if source == AudioSource::System {
            ready.extend(recording.assembler.system_heard(end));
        }
        self.save_segments(ready);
        self.finish_recording_if_done();
    }

    fn save_segments(&mut self, segments: Vec<MeetingSegment>) {
        if segments.is_empty() {
            return;
        }
        let data = self.data();
        let State::Recording(recording) = &mut self.meetings.state else { return };
        meetings::append(&data, &recording.meeting.id, &segments);
        recording.meeting.words += segments.iter().map(|s| word_count(&s.text)).sum::<usize>();
        meetings::save(&data, &recording.meeting);
        let _ = self.window.send(Event::DataChanged);
    }

    /// After Stop, once every piece is back: keep the meeting (and write its notes) or drop
    /// it if it was too short.
    fn finish_recording_if_done(&mut self) {
        let State::Recording(recording) = &mut self.meetings.state else { return };
        if !recording.stopping || recording.in_flight > 0 {
            return;
        }
        let rest = recording.assembler.finish();
        self.save_segments(rest);
        let State::Recording(mut recording) = std::mem::take(&mut self.meetings.state) else { return };
        recording.meeting.ended = Some(SystemTime::now());
        let blocked = recording.capture.system_audio_seems_blocked();
        if blocked {
            log::warn!("The computer's sound was silent throughout; it probably wasn't being recorded");
            recording.meeting.sources = vec![AudioSource::Mic];
        }
        let data = self.data();
        if recording.meeting.duration(SystemTime::now()) < MINIMUM_LENGTH || recording.meeting.words == 0 {
            log::info!("Meeting too short or silent; not kept");
            meetings::delete(&data, &recording.meeting.id);
            self.flash("Too short to keep");
        } else {
            let segments = meetings::segments(&data, &recording.meeting.id);
            self.begin_notes(recording.meeting, segments);
        }
        self.refresh_tray();
        let _ = self.window.send(Event::DataChanged);
        self.send_status();
    }

    // MARK: Notes

    fn begin_notes(&mut self, mut meeting: Meeting, segments: Vec<MeetingSegment>) {
        meeting.status = MeetingStatus::Writing;
        meetings::save(&self.data(), &meeting);
        let transcript = meeting::transcript_text(&segments, meeting.heard_others());
        let words = word_count(&transcript);
        let parts = meeting_notes::parts(&transcript, meeting_notes::PART_WORDS);
        let writing = Writing { meeting, segments, parts, summaries: Vec::new(), words };
        // No writer model, or too little said for a summary: notes by the rules.
        if self.smart != ModelStatus::Ready || words < 25 {
            let notes = meeting_notes::extractive(&writing.segments);
            self.meetings.state = State::Writing(Box::new(writing));
            self.complete_notes(None, notes);
            return;
        }
        self.meetings.state = State::Writing(Box::new(writing));
        self.next_notes_step();
    }

    fn next_notes_step(&mut self) {
        let State::Writing(writing) = &self.meetings.state else { return };
        let id = writing.meeting.id.clone();
        let step = writing.summaries.len();
        let job = if writing.parts.len() > 1 && step < writing.parts.len() {
            let user = meeting_notes::part_prompt(&writing.parts[step], step, writing.parts.len());
            MeetingJob::Chat { id, step, system: meeting_notes::PART_INSTRUCTIONS, user, max_tokens: 320 }
        } else {
            let user = if writing.parts.len() > 1 {
                meeting_notes::notes_prompt_from_parts(&writing.summaries)
            } else {
                meeting_notes::notes_prompt(&writing.parts.join("\n"))
            };
            MeetingJob::Chat { id, step, system: meeting_notes::NOTES_INSTRUCTIONS, user, max_tokens: 520 }
        };
        let _ = self.meeting_jobs.send(job);
        self.send_status();
    }

    /// The writer's reply for one step of the notes.
    pub(super) fn meeting_chat(&mut self, id: String, step: usize, reply: Option<String>) {
        let State::Writing(writing) = &mut self.meetings.state else { return };
        if writing.meeting.id != id || writing.summaries.len() != step {
            return;
        }
        let final_step = writing.parts.len() <= 1 || step >= writing.parts.len();
        let Some(reply) = reply else {
            let notes = meeting_notes::extractive(&writing.segments);
            return self.complete_notes(None, notes);
        };
        if !final_step {
            writing.summaries.push(reply.trim().to_string());
            return self.next_notes_step();
        }
        match meeting_notes::parse(&reply, NotesGenerator::KuduWriter) {
            Some((title, notes)) if meeting_notes::is_usable(&notes, writing.words) => self.complete_notes(title, notes),
            _ => {
                log::info!("The writer's meeting notes weren't usable; using the built-in notes");
                let notes = meeting_notes::extractive(&writing.segments);
                self.complete_notes(None, notes);
            }
        }
    }

    fn complete_notes(&mut self, title: Option<String>, notes: MeetingNotes) {
        let State::Writing(writing) = std::mem::take(&mut self.meetings.state) else { return };
        let mut meeting = writing.meeting;
        if let Some(title) = title.filter(|t| !t.is_empty()) {
            meeting.title = title;
        }
        log::info!("Meeting notes written ({:?}, {} words of transcript)", notes.generator, writing.words);
        meeting.notes = Some(notes);
        meeting.status = MeetingStatus::Done;
        meetings::save(&self.data(), &meeting);
        self.flash("Meeting notes saved");
        let _ = self.window.send(Event::DataChanged);
        self.send_status();
        self.next_queued();
    }

    /// Writes the notes of a saved meeting again.
    pub fn regenerate_notes(&mut self, id: String) {
        if !self.meetings.queue.contains(&id) {
            self.meetings.queue.push_back(id);
        }
        self.next_queued();
    }

    fn next_queued(&mut self) {
        if !matches!(self.meetings.state, State::Idle) {
            return;
        }
        let data = self.data();
        while let Some(id) = self.meetings.queue.pop_front() {
            let Some(mut meeting) = meetings::load(&data, &id) else { continue };
            let segments = meetings::segments(&data, &id);
            if segments.is_empty() {
                continue;
            }
            if meeting.ended.is_none() {
                let last = segments.iter().map(|s| s.end).fold(0.0, f64::max);
                meeting.ended = Some(meeting.started + Duration::from_secs_f64(last));
            }
            meeting.words = segments.iter().map(|s| word_count(&s.text)).sum();
            self.begin_notes(meeting, segments);
            return;
        }
    }

    /// Meetings left recording or half-written when Kudu last quit keep their transcript and
    /// get their notes now.
    fn recover_interrupted(&mut self) {
        let data = self.data();
        for meeting in meetings::list(&data).into_iter().filter(|m| m.status != MeetingStatus::Done) {
            if meetings::segments(&data, &meeting.id).is_empty() {
                meetings::delete(&data, &meeting.id);
            } else {
                log::info!("Finishing a meeting interrupted by a quit");
                self.meetings.queue.push_back(meeting.id);
            }
        }
        self.next_queued();
    }

    pub fn delete_meeting(&mut self, id: &str) {
        let busy = match &self.meetings.state {
            State::Recording(recording) => recording.meeting.id == id,
            State::Writing(writing) => writing.meeting.id == id,
            State::Idle => false,
        };
        if busy {
            return;
        }
        self.meetings.queue.retain(|queued| queued != id);
        meetings::delete(&self.data(), id);
        let _ = self.window.send(Event::DataChanged);
    }

    pub fn delete_all_meetings(&mut self) {
        let ids: Vec<String> = meetings::list(&self.data()).into_iter().map(|m| m.id).collect();
        for id in ids {
            self.delete_meeting(&id);
        }
    }

    // MARK: Detection

    fn check_for_meetings(&mut self) {
        let settings = &self.store.settings.meetings;
        if !settings.enabled || settings.auto_start == MeetingAutoStart::Off {
            return;
        }
        if self.meetings.last_check.is_some_and(|last| last.elapsed() < DETECTION_INTERVAL) {
            return;
        }
        self.meetings.last_check = Some(Instant::now());
        let platform = self.platform.clone();
        let apps = platform.meeting_apps_using_microphone();
        let available = apps.is_some();
        if available != self.meetings.detection_available {
            self.meetings.detection_available = available;
            self.send_status();
        }
        let Some(change) = self.meetings.detector.observe(&apps.unwrap_or_default(), Instant::now()) else { return };
        log::info!("Meeting {change:?}");
        match change {
            MeetingChange::Started(app) => {
                if !matches!(self.meetings.state, State::Idle) {
                    return;
                }
                match self.store.settings.meetings.auto_start {
                    MeetingAutoStart::Automatic => self.start_meeting(Some(app.name), true),
                    MeetingAutoStart::Ask => {
                        let key = self.shortcuts().meeting.map(|s| s.display(self.platform.notation()));
                        let hint = match key {
                            Some(key) => format!("Meeting in {} · {key} to take notes", app.name),
                            None => format!("Meeting in {} · take notes from the tray", app.name),
                        };
                        self.platform.overlay().show(OverlayState::Message(hint));
                    }
                    MeetingAutoStart::Off => {}
                }
            }
            MeetingChange::Ended => {
                if let State::Recording(recording) = &self.meetings.state {
                    if recording.from_detection {
                        self.stop_meeting();
                    }
                }
            }
        }
    }

    /// The detection follows the settings (turned off, or set to do nothing).
    pub(super) fn meeting_settings_changed(&mut self) {
        let settings = &self.store.settings.meetings;
        if !settings.enabled || settings.auto_start == MeetingAutoStart::Off {
            self.meetings.detector.reset();
        }
        if !settings.enabled && self.meeting_is_recording() {
            self.stop_meeting();
        }
    }
}
