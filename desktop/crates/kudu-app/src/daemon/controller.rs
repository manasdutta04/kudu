//! Runs the hold → speak → release → text-at-cursor loop: reacts to shortcuts, records,
//! sends audio to the worker, inserts the result, keeps history and learns from corrections.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use kudu_core::shortcut::{HotkeyEvent, Matcher, Recorded, Shortcut, Shortcuts};
use kudu_core::{
    category_for, term_extractor, AppCategory, DictationRecord, DictionaryEntry, DictionaryStage, LearnedCorrection,
    Placeholders, SnippetStage, TextContext, TextPipeline,
};
use kudu_engine::audio::LevelCallback;
use kudu_engine::resample::resample;
use kudu_engine::segmenter::{Segmenter, OVERLAP_SECONDS};
use kudu_engine::sounds::{self, Cue};
use kudu_engine::{Recorder, SAMPLE_RATE};
use parking_lot::Mutex;

use super::learning::EditObserver;
use super::levels::Levels;
use super::store::Store;
use super::worker::{DictationPlan, Finished, Job, Outcome};
use crate::ipc::{Event, ModelStatus, ShortcutSlot};
use crate::overlay::OverlayState;
use crate::platform::{FocusedApp, Platform, TrayState};
use crate::settings::RecordingMode;

/// Holds shorter than this are taps (hands-free gestures), not dictations.
const MINIMUM_HOLD: Duration = Duration::from_millis(250);
/// Two taps within this window lock hands-free recording.
const DOUBLE_TAP: Duration = Duration::from_millis(400);

struct Active {
    session: u64,
    started: Instant,
    command: bool,
    hands_free: bool,
    category: AppCategory,
    plan: Option<Box<DictationPlan>>,
    /// Device-rate sample index of the last cut sent to the worker.
    cut: usize,
}

pub struct Controller {
    pub platform: Arc<dyn Platform>,
    pub store: Store,
    pub matcher: Arc<Mutex<Matcher>>,
    pub speech: ModelStatus,
    pub smart: ModelStatus,
    /// Whether a newer Kudu is out (see `daemon::Updates`).
    pub update: crate::ipc::UpdateState,
    pub hotkeys_active: bool,
    jobs: Sender<Job>,
    learned: Sender<Vec<LearnedCorrection>>,
    window: Sender<Event>,
    recorder: Recorder,
    active: Option<Active>,
    sessions: u64,
    busy: bool,
    last_short_tap: Option<Instant>,
    ignore_next_release: bool,
    observer: EditObserver,
    /// The dictionary before the last learned words, for Undo.
    undo_learned: Option<(Vec<DictionaryEntry>, i64)>,
    recording_slot: Option<ShortcutSlot>,
    /// Microphone names, refreshed when the window opens (listing devices is slow on Windows).
    microphones: Arc<Mutex<Vec<String>>>,
}

impl Controller {
    pub fn new(
        platform: Arc<dyn Platform>,
        store: Store,
        jobs: Sender<Job>,
        learned: Sender<Vec<LearnedCorrection>>,
        window: Sender<Event>,
    ) -> Self {
        let shortcuts = store.settings.shortcuts.clone().unwrap_or_else(|| platform.default_shortcuts());
        let matcher = Arc::new(Mutex::new(Matcher::new(shortcuts, platform.escape_code())));
        let mut recorder = Recorder::new();
        recorder.set_device(store.settings.microphone.clone());
        Self {
            platform,
            store,
            matcher,
            speech: ModelStatus::Missing,
            smart: ModelStatus::Missing,
            update: crate::ipc::UpdateState::Idle,
            hotkeys_active: false,
            jobs,
            learned,
            window,
            recorder,
            active: None,
            sessions: 0,
            busy: false,
            last_short_tap: None,
            ignore_next_release: false,
            observer: EditObserver::default(),
            undo_learned: None,
            recording_slot: None,
            microphones: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn shortcuts(&self) -> Shortcuts {
        self.matcher.lock().shortcuts.clone()
    }

    pub fn is_recording(&self) -> bool {
        self.active.is_some()
    }

    /// Settings changed on disk (the window saved them).
    pub fn settings_reloaded(&mut self) {
        let shortcuts = self.store.settings.shortcuts.clone().unwrap_or_else(|| self.platform.default_shortcuts());
        if self.matcher.lock().shortcuts != shortcuts {
            self.matcher.lock().set_shortcuts(shortcuts);
        }
        self.recorder.set_device(self.store.settings.microphone.clone());
    }

    // MARK: - Shortcuts

    pub fn hotkey(&mut self, event: HotkeyEvent) {
        match event {
            HotkeyEvent::Pressed => {
                if self.active.as_ref().is_some_and(|a| a.hands_free) {
                    // Tap while locked: stop and transcribe. Ignore this press's release.
                    self.ignore_next_release = true;
                    self.finish();
                } else {
                    self.begin(false);
                }
            }
            HotkeyEvent::Released => {
                if self.ignore_next_release {
                    self.ignore_next_release = false;
                } else if self.active.as_ref().is_some_and(|a| !a.hands_free && !a.command) {
                    self.handle_release();
                }
            }
            HotkeyEvent::Cancelled => {
                self.ignore_next_release = false;
                self.cancel();
            }
            HotkeyEvent::CommandPressed => {
                match &mut self.active {
                    // The dictation hold became a Command mode instruction.
                    Some(active) if !active.hands_free => {
                        active.command = true;
                        let _ = self.jobs.send(Job::Cancel { session: active.session });
                        self.platform.overlay().show(OverlayState::Recording { locked: false, command: true });
                    }
                    Some(_) => {}
                    None => self.begin(true),
                }
            }
            HotkeyEvent::CommandReleased => {
                if self.active.as_ref().is_some_and(|a| a.command) {
                    self.finish();
                }
            }
            HotkeyEvent::PasteLast => self.paste_last(),
        }
    }

    fn handle_release(&mut self) {
        let Some(active) = &mut self.active else { return };
        let now = Instant::now();
        if now - active.started >= MINIMUM_HOLD {
            self.finish();
            return;
        }
        match self.store.settings.recording_mode {
            RecordingMode::TapToToggle => self.lock_hands_free(),
            RecordingMode::HoldOrDoubleTap => {
                if self.last_short_tap.is_some_and(|tap| now - tap < DOUBLE_TAP) {
                    self.last_short_tap = None;
                    self.lock_hands_free();
                } else {
                    self.last_short_tap = Some(now);
                    // A lone short tap: nothing was said yet; drop it quietly.
                    self.cancel_quietly();
                }
            }
            RecordingMode::HoldOnly => self.cancel_quietly(),
        }
    }

    fn lock_hands_free(&mut self) {
        if let Some(active) = &mut self.active {
            active.hands_free = true;
            self.platform.overlay().show(OverlayState::Recording { locked: true, command: active.command });
        }
    }

    // MARK: - Recording

    fn begin(&mut self, command: bool) {
        if self.active.is_some() || self.busy {
            return;
        }
        self.observer.cancel();
        match &self.speech {
            ModelStatus::Ready => {}
            ModelStatus::Loading => return self.flash("Loading the speech model…"),
            ModelStatus::Downloading { .. } => return self.flash("Downloading the speech model…"),
            _ => return self.flash("Finish setting up Kudu first"),
        }
        let overlay_platform = self.platform.clone();
        let levels = Arc::new(Mutex::new(Levels::default()));
        let on_level: LevelCallback = Box::new(move |rms| {
            let history = levels.lock().push(rms);
            overlay_platform.overlay().set_levels(history);
        });
        if let Err(error) = self.recorder.start(on_level) {
            log::error!("Couldn't start the microphone: {error:#}");
            return self.flash("Microphone unavailable");
        }
        if self.store.settings.sounds {
            sounds::play(Cue::Start);
        }
        self.matcher.lock().is_dictating = true;
        self.sessions += 1;
        let target = self.platform.focused_app();
        let category = target
            .as_ref()
            .map(|app| category_for(&app.id, &app.name, &app.title))
            .unwrap_or(AppCategory::General);
        let plan = (!command).then(|| Box::new(self.plan(category, target.as_ref())));
        log::debug!("Dictating into {:?} ({category:?})", target.as_ref().map(|t| &t.id));
        self.active = Some(Active {
            session: self.sessions,
            started: Instant::now(),
            command,
            hands_free: false,
            category,
            plan,
            cut: 0,
        });
        self.platform.overlay().show(OverlayState::Recording { locked: false, command });
        self.platform.set_tray_state(TrayState::Recording, "Kudu is listening");
        self.send_status();
    }

    /// The cleanup and formatting for this dictation, from the current settings. Screen terms
    /// and the text before the cursor are read on a helper thread while the user speaks.
    fn plan(&self, category: AppCategory, target: Option<&FocusedApp>) -> DictationPlan {
        let cleanup = &self.store.cleanup;
        let style = cleanup.styles.style_for(category);
        let placeholders = Placeholders {
            clipboard: Arc::new(|| arboard::Clipboard::new().ok().and_then(|mut c| c.get_text().ok())),
            calendar: self.store.calendar.clone(),
            ..Placeholders::default()
        };
        let screen_terms = Arc::new(Mutex::new(Vec::new()));
        let field_before = Arc::new(Mutex::new(None));
        let (platform, terms, before) = (self.platform.clone(), screen_terms.clone(), field_before.clone());
        let read_screen = self.store.settings.screen_context;
        // Accessibility reads can take a moment; do them while the user speaks.
        std::thread::Builder::new()
            .name("kudu-context".into())
            .spawn(move || {
                *before.lock() = platform.text_before_cursor();
                if read_screen {
                    let texts = platform.screen_text(Duration::from_millis(150));
                    *terms.lock() = term_extractor::terms(&texts);
                }
            })
            .ok();
        let context = TextContext::new(category, None, style);
        DictationPlan {
            before: TextPipeline::before_formatting(cleanup, &self.store.dictionary, &[], Placeholders::default()),
            after: TextPipeline::after_formatting(cleanup),
            snippets: SnippetStage::new(&self.store.snippets, placeholders),
            dictionary: DictionaryStage::new(&self.store.dictionary),
            dictionary_terms: self.store.dictionary.iter().map(|e| e.written.clone()).collect(),
            screen_terms,
            context,
            strength: cleanup.editing_strength,
            field_before,
            app_name: target.map(|t| t.name.clone()).filter(|n| !n.is_empty()),
        }
    }

    /// Called a few times a second while recording: sends finished phrases to the worker, so
    /// only the last few seconds are left when the user lets go.
    pub fn tick(&mut self) {
        let Some(active) = &mut self.active else { return };
        if active.command {
            return;
        }
        let (samples, rate) = self.recorder.read_since(0);
        let segmenter = Segmenter::new(rate);
        if let Some(cut) = segmenter.next_cut(&samples, active.cut) {
            let overlap = if active.cut == 0 { 0 } else { ((OVERLAP_SECONDS * rate as f32) as usize).min(active.cut) };
            let start = active.cut - overlap;
            let piece = resample(&samples[start..cut], rate, SAMPLE_RATE);
            let _ = self.jobs.send(Job::Piece { session: active.session, samples: piece, overlap: overlap as f32 / rate as f32 });
            active.cut = cut;
        }
    }

    fn finish(&mut self) {
        let Some(active) = self.active.take() else { return };
        let released = Instant::now();
        let (samples, rate) = self.recorder.stop_raw();
        self.matcher.lock().is_dictating = false;
        if self.store.settings.sounds {
            sounds::play(Cue::Stop);
        }
        let audio_seconds = samples.len() as f64 / rate as f64;
        if kudu_core::audio_levels::is_silent(&samples) {
            self.platform.overlay().show(OverlayState::Message("No sound from the microphone".into()));
            self.idle();
            return;
        }
        self.busy = true;
        self.platform.overlay().show(OverlayState::Transcribing { command: active.command });
        self.platform.set_tray_state(TrayState::Working, "Kudu is writing");
        if active.command {
            // Copy the selection now that the keys are up, while the instruction is transcribed.
            let selection = Arc::new(Mutex::new(None));
            let (platform, slot) = (self.platform.clone(), selection.clone());
            std::thread::spawn(move || *slot.lock() = Some(platform.copy_selection()));
            let _ = self.jobs.send(Job::Command {
                samples: resample(&samples, rate, SAMPLE_RATE),
                selection,
                category: active.category,
                released,
                audio_seconds,
            });
        } else {
            let overlap = if active.cut == 0 { 0 } else { ((OVERLAP_SECONDS * rate as f32) as usize).min(active.cut) };
            let tail = resample(&samples[active.cut - overlap..], rate, SAMPLE_RATE);
            let _ = self.jobs.send(Job::Finish {
                session: active.session,
                samples: tail,
                overlap: overlap as f32 / rate as f32,
                plan: active.plan.expect("dictations have a plan"),
                released,
                audio_seconds,
            });
        }
        self.send_status();
    }

    fn cancel(&mut self) {
        if let Some(active) = self.active.take() {
            self.recorder.cancel();
            let _ = self.jobs.send(Job::Cancel { session: active.session });
            self.matcher.lock().is_dictating = false;
            self.platform.overlay().hide();
            self.idle();
        }
    }

    /// Drops a too-short recording without any message.
    fn cancel_quietly(&mut self) {
        self.cancel();
    }

    /// Refreshes the tray and the window's status after a model or setting changed.
    pub fn idle_status(&mut self) {
        if self.active.is_none() && !self.busy {
            let state = if self.speech == ModelStatus::Ready { TrayState::Idle } else { TrayState::NeedsSetup };
            self.platform.set_tray_state(state, &self.tooltip());
        }
        self.send_status();
    }

    fn idle(&mut self) {
        self.busy = false;
        let state = if self.speech == ModelStatus::Ready { TrayState::Idle } else { TrayState::NeedsSetup };
        self.platform.set_tray_state(state, &self.tooltip());
        self.send_status();
    }

    pub fn tooltip(&self) -> String {
        let key = self.shortcuts().dictate.display(self.platform.notation());
        match &self.speech {
            ModelStatus::Ready => format!("Kudu is ready · hold {key} to dictate"),
            ModelStatus::Downloading { fraction } => format!("Downloading the speech model… {}%", (fraction * 100.0) as u32),
            ModelStatus::Loading => "Loading the speech model…".into(),
            ModelStatus::Missing => "Finish setting up Kudu".into(),
            ModelStatus::Failed { .. } => "Speech model error: open Kudu".into(),
        }
    }

    // MARK: - Results

    pub fn worker_outcome(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::SpeechLoaded(result) => {
                self.speech = match result {
                    Ok(_) => ModelStatus::Ready,
                    Err(message) => {
                        log::error!("Speech model failed to load: {message}");
                        ModelStatus::Failed { message }
                    }
                };
                self.idle();
            }
            Outcome::SmartLoaded(result) => {
                self.smart = match result {
                    Ok(_) => ModelStatus::Ready,
                    Err(message) => ModelStatus::Failed { message },
                };
                self.send_status();
            }
            Outcome::Failed(message) => {
                // A model that crashed while loading shouldn't look like it's still loading.
                if self.speech == ModelStatus::Loading {
                    self.speech = ModelStatus::Failed { message: message.clone() };
                }
                if self.smart == ModelStatus::Loading {
                    self.smart = ModelStatus::Failed { message: message.clone() };
                }
                self.platform.overlay().show(OverlayState::Message(message));
                self.idle();
            }
            Outcome::NothingHeard => {
                self.platform.overlay().show(OverlayState::Message("Didn't catch that".into()));
                self.idle();
            }
            Outcome::Dictation(finished) => self.insert(finished),
            Outcome::Command(Ok(finished)) => self.insert(finished),
            Outcome::Command(Err(message)) => {
                if self.store.settings.sounds {
                    sounds::play(Cue::Error);
                }
                self.platform.overlay().show(OverlayState::Message(message));
                self.idle();
            }
        }
    }

    fn insert(&mut self, finished: Finished) {
        self.platform.overlay().hide();
        let text = finished.text.clone();
        if text.trim().is_empty() {
            return self.idle();
        }
        let platform = self.platform.clone();
        if let Err(error) = platform.insert_text(&text) {
            log::error!("Couldn't insert the text: {error:#}");
            self.platform.overlay().show(OverlayState::Message("Couldn't type here: copied to the clipboard".into()));
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(text.clone());
            }
        }
        let latency = finished.released.elapsed().as_millis() as u64;
        let app_id = self.platform.focused_app().map(|a| a.id);
        let mut record = DictationRecord::new(finished.raw, text.clone(), finished.audio_seconds, latency)
            .with_formatted(finished.formatted)
            .with_dictionary_fixes(finished.dictionary_fixes);
        if let Some(app) = app_id {
            record = record.with_bundle_id(app);
        }
        if finished.command {
            record = record.as_command();
        }
        self.store.record(&record, finished.category);
        log::info!("Dictation: {:.1}s audio → {latency} ms (formatted: {})", finished.audio_seconds, finished.formatted);
        if self.store.settings.learn_from_corrections && !finished.command {
            let known = self.store.dictionary.iter().map(|e| e.written.clone()).collect();
            self.observer.watch(self.platform.clone(), text, known, self.learned.clone());
        }
        let _ = self.window.send(Event::DataChanged);
        self.idle();
    }

    pub fn paste_last(&mut self) {
        match self.store.last_transcript() {
            Some(text) => {
                if let Err(error) = self.platform.insert_text(&text) {
                    log::error!("Couldn't paste the last transcript: {error:#}");
                }
            }
            None => self.flash("Nothing dictated yet"),
        }
    }

    // MARK: - Learning

    /// Saves learned spellings to the dictionary: a new entry, or one more way of hearing an
    /// existing spelling.
    pub fn learned(&mut self, corrections: Vec<LearnedCorrection>) {
        let before = self.store.dictionary.clone();
        for correction in &corrections {
            match self.store.dictionary.iter_mut().find(|e| e.written == correction.written) {
                Some(entry) => {
                    if !entry.heard.iter().any(|h| h.to_lowercase() == correction.heard) {
                        entry.heard.push(correction.heard.clone());
                    }
                }
                None => self
                    .store
                    .dictionary
                    .push(DictionaryEntry::new(vec![correction.heard.clone()], correction.written.clone(), true)),
            }
        }
        self.store.save_dictionary();
        self.store.record_learned(corrections.len() as i64);
        self.undo_learned = Some((before, corrections.len() as i64));
        log::info!("Learned {} spelling(s)", corrections.len());
        if self.active.is_none() && !self.busy {
            self.platform.overlay().show(OverlayState::Learned(corrections.iter().map(|c| c.written.clone()).collect()));
        }
        let _ = self.window.send(Event::DataChanged);
    }

    pub fn undo_learned(&mut self) {
        if let Some((before, count)) = self.undo_learned.take() {
            self.store.dictionary = before;
            self.store.save_dictionary();
            self.store.record_learned(-count);
            self.platform.overlay().show(OverlayState::Message("Removed from dictionary".into()));
            let _ = self.window.send(Event::DataChanged);
        }
    }

    // MARK: - Recording shortcuts (from Settings)

    pub fn record_shortcut(&mut self, slot: ShortcutSlot) {
        self.recording_slot = Some(slot);
        self.platform.record_shortcut();
    }

    pub fn cancel_shortcut_recording(&mut self) {
        self.recording_slot = None;
        self.platform.cancel_recording();
    }

    pub fn recorded(&mut self, result: Recorded) {
        let Some(slot) = self.recording_slot.take() else { return };
        let shortcut = match result {
            Recorded::Done(shortcut) => shortcut,
            Recorded::Cancelled | Recorded::Pending => {
                let _ = self.window.send(Event::Recorded { slot, shortcut: None, error: None });
                return;
            }
        };
        let (shortcut, error) = match self.apply(slot, shortcut.clone()) {
            Ok(()) => (Some(shortcut), None),
            Err(error) => (None, Some(error)),
        };
        let _ = self.window.send(Event::Recorded { slot, shortcut, error });
    }

    fn apply(&mut self, slot: ShortcutSlot, shortcut: Shortcut) -> Result<(), String> {
        let notation = self.platform.notation();
        if slot != ShortcutSlot::PasteLast {
            let platform = self.platform.clone();
            if !shortcut.is_usable_for_holding(|code| platform.key_types_text(code)) {
                return Err(format!(
                    "{} types text, so it can't be held to talk. Add Ctrl, Alt or the Windows/Super key, or use an F-key.",
                    shortcut.display(notation)
                ));
            }
        } else if shortcut.is_modifier_only() {
            return Err("Paste last transcript needs a key, like Ctrl + Alt + V.".into());
        }
        let mut shortcuts = self.shortcuts();
        match slot {
            ShortcutSlot::Dictate => shortcuts.dictate = shortcut,
            ShortcutSlot::Command => shortcuts.command = Some(shortcut),
            ShortcutSlot::PasteLast => shortcuts.paste_last = Some(shortcut),
        }
        if let Some(problem) = shortcuts.problems().into_iter().next() {
            return Err(problem);
        }
        self.matcher.lock().set_shortcuts(shortcuts.clone());
        self.store.settings.shortcuts = Some(shortcuts);
        self.store.save_settings();
        self.platform.set_tray_state(TrayState::Idle, &self.tooltip());
        Ok(())
    }

    // MARK: - Status

    fn flash(&self, message: &str) {
        self.platform.overlay().show(OverlayState::Message(message.into()));
    }

    /// Lists the microphones again in the background, then updates the window.
    pub fn refresh_microphones(&self) {
        let (microphones, window) = (self.microphones.clone(), self.window.clone());
        let status = self.status();
        std::thread::Builder::new()
            .name("kudu-devices".into())
            .spawn(move || {
                let names = Recorder::input_devices();
                if *microphones.lock() != names {
                    *microphones.lock() = names.clone();
                    let _ = window.send(Event::Status(crate::ipc::Status { microphones: names, ..status }));
                }
            })
            .ok();
    }

    pub fn send_status(&self) {
        let _ = self.window.send(Event::Status(self.status()));
    }

    pub fn status(&self) -> crate::ipc::Status {
        crate::ipc::Status {
            speech: self.speech.clone(),
            smart: self.smart.clone(),
            hotkeys_active: self.hotkeys_active,
            recording: self.active.is_some(),
            launch_at_login: self.platform.launch_at_login(),
            microphones: self.microphones.lock().clone(),
            issues: self
                .platform
                .setup_issues()
                .into_iter()
                .map(|i| crate::ipc::Issue { title: i.title, detail: i.detail, command: i.command })
                .collect(),
            platform: if cfg!(windows) { "windows".into() } else { "linux".into() },
            update: self.update.clone(),
        }
    }
}
