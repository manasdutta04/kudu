//! The worker thread: owns the speech model and the small language model, and turns audio
//! into finished text. Jobs run in order, so the pieces of a dictation transcribed while the
//! user is still speaking are always stitched before its final part.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use kudu_core::{
    command_output, AppCategory, DictionaryStage, EditingStrength, SnippetStage, TermSpeller,
    TextContext, TextPipeline, TextStage,
};
use kudu_engine::segmenter::{stitch, stitch_timed};
use kudu_engine::{SpeechEngine, SpeechModel, WriterModel};
use parking_lot::Mutex;

use super::vocabulary;

/// Everything needed to finish a dictation, built from the settings when recording starts.
pub struct DictationPlan {
    pub before: TextPipeline,
    pub after: TextPipeline,
    pub snippets: SnippetStage,
    pub dictionary: DictionaryStage,
    pub dictionary_terms: Vec<String>,
    /// Names and terms on screen; filled in by a helper thread while the user speaks.
    pub screen_terms: Arc<Mutex<Vec<String>>>,
    pub context: TextContext,
    #[cfg_attr(not(feature = "smart"), allow(dead_code))]
    pub strength: EditingStrength,
    /// The text before the cursor; filled in by a helper thread while the user speaks.
    pub field_before: Arc<Mutex<Option<String>>>,
    #[cfg_attr(not(feature = "smart"), allow(dead_code))]
    pub app_name: Option<String>,
}

pub enum Job {
    LoadSpeech { model: SpeechModel, models: PathBuf },
    LoadSmart { model: WriterModel, models: PathBuf },
    /// A finished phrase of the current dictation (16 kHz), whose first `overlap` seconds were
    /// already part of the previous piece.
    Piece { session: u64, samples: Vec<f32>, overlap: f32 },
    /// The rest of the dictation: transcribe, stitch, clean up, format.
    Finish { session: u64, samples: Vec<f32>, overlap: f32, plan: Box<DictationPlan>, released: Instant, audio_seconds: f64 },
    /// A Command mode instruction. `selection` is filled by a helper thread copying the
    /// selected text (None inside = nothing selected).
    Command { samples: Vec<f32>, selection: Arc<Mutex<Option<Option<String>>>>, category: AppCategory, released: Instant, audio_seconds: f64 },
    /// Forget a cancelled dictation.
    Cancel { session: u64 },
}

pub enum Outcome {
    SpeechLoaded(Result<SpeechModel, String>),
    SmartLoaded(Result<WriterModel, String>),
    Dictation(Finished),
    Command(Result<Finished, String>),
    /// The speech model heard nothing.
    NothingHeard,
    /// A job failed unexpectedly.
    Failed(String),
}

pub struct Finished {
    pub raw: String,
    pub text: String,
    pub formatted: bool,
    pub dictionary_fixes: usize,
    pub audio_seconds: f64,
    pub released: Instant,
    pub command: bool,
    pub category: AppCategory,
}

pub fn spawn(results: Sender<Outcome>) -> Sender<Job> {
    let (jobs, receiver) = crossbeam_channel::unbounded();
    std::thread::Builder::new()
        .name("kudu-worker".into())
        .spawn(move || Worker::default().run(receiver, results))
        .expect("worker thread");
    jobs
}

#[derive(Default)]
struct Worker {
    speech: Option<SpeechEngine>,
    #[cfg(feature = "smart")]
    smart: Option<kudu_engine::smart::SmartFormatter>,
    session: u64,
    text: String,
}

impl Worker {
    fn run(mut self, jobs: Receiver<Job>, results: Sender<Outcome>) {
        vocabulary::preload();
        for job in jobs {
            // A crash in one job (a model that won't load, an odd recording) is reported and
            // the worker carries on with the next one.
            let outcome = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.handle(job))) {
                Ok(outcome) => outcome,
                Err(_) => {
                    self.text.clear();
                    Some(Outcome::Failed("Something went wrong; see kudu.log".into()))
                }
            };
            if let Some(outcome) = outcome {
                if results.send(outcome).is_err() {
                    break;
                }
            }
        }
    }

    fn handle(&mut self, job: Job) -> Option<Outcome> {
        match job {
            Job::LoadSpeech { model, models } => {
                self.speech = None;
                Some(Outcome::SpeechLoaded(
                    SpeechEngine::load(model, &models).map(|engine| {
                        self.speech = Some(engine);
                        model
                    })
                    .map_err(|e| format!("{e:#}")),
                ))
            }
            Job::LoadSmart { model, models } => Some(Outcome::SmartLoaded(self.load_smart(model, models))),
            Job::Piece { session, samples, overlap } => {
                self.join(session, &samples, overlap);
                None
            }
            Job::Cancel { session } => {
                if self.session == session {
                    self.text.clear();
                }
                None
            }
            Job::Finish { session, samples, overlap, plan, released, audio_seconds } => {
                self.join(session, &samples, overlap);
                let raw = std::mem::take(&mut self.text);
                if raw.trim().is_empty() {
                    return Some(Outcome::NothingHeard);
                }
                Some(Outcome::Dictation(self.finish(raw, &plan, released, audio_seconds)))
            }
            Job::Command { samples, selection, category, released, audio_seconds } => {
                let instruction = self.speech.as_ref().map(|s| s.transcribe(&samples)).unwrap_or_default();
                let instruction = kudu_core::stages::FillerRemover::new().apply(&instruction, &TextContext::default());
                if instruction.trim().is_empty() {
                    return Some(Outcome::NothingHeard);
                }
                Some(Outcome::Command(self.command(instruction, &selection, category, released, audio_seconds)))
            }
        }
    }

    /// Adds a piece of audio to the current dictation's text.
    fn join(&mut self, session: u64, samples: &[f32], overlap: f32) {
        if session != self.session {
            self.session = session;
            self.text.clear();
        }
        let Some(speech) = &self.speech else { return };
        if overlap <= 0.0 && self.text.is_empty() {
            self.text = speech.transcribe(samples);
            return;
        }
        let (plain, words) = speech.transcribe_words(samples);
        self.text = match words {
            Some(words) => stitch_timed(&self.text, &words, overlap),
            None => stitch(&self.text, &plain),
        };
    }

    fn finish(&self, raw: String, plan: &DictationPlan, released: Instant, audio_seconds: f64) -> Finished {
        let category = plan.context.category;
        // Spacing and capitals fit the character before the cursor.
        let mut context = plan.context;
        context.preceding_character = plan.field_before.lock().as_deref().and_then(|t| t.chars().last());
        let context = &context;
        // A snippet replaces the whole dictation.
        if let Some(expansion) = plan.snippets.expansion_for(&raw) {
            let text = plan.after.process(&expansion, context);
            return Finished { raw, text, formatted: false, dictionary_fixes: 0, audio_seconds, released, command: false, category };
        }
        let cleaned = plan.before.process(&raw, context);
        let (mut text, formatted) = self.format(&cleaned, plan);
        let screen = plan.screen_terms.lock().clone();
        let speller = TermSpeller::new(plan.dictionary_terms.iter().cloned(), screen, vocabulary::is_known_word);
        let (spelled, fixes) = speller.apply(&text);
        text = plan.after.process(&spelled, context);
        let dictionary_fixes = plan.dictionary.corrections(&raw) + fixes;
        Finished { raw, text, formatted, dictionary_fixes, audio_seconds, released, command: false, category }
    }

    #[cfg(feature = "smart")]
    fn format(&self, text: &str, plan: &DictationPlan) -> (String, bool) {
        use kudu_core::formatting_need;
        let Some(smart) = &self.smart else { return (text.to_string(), false) };
        let category = plan.context.category;
        let wanted = if category == AppCategory::Code {
            formatting_need::should_format_code(text, plan.strength)
        } else {
            formatting_need::should_format(text, plan.strength, category)
        };
        if !wanted {
            return (text.to_string(), false);
        }
        let request = kudu_engine::smart::FormatRequest {
            text: text.to_string(),
            category,
            style: plan.context.style,
            strength: plan.strength,
            terms: plan.dictionary_terms.clone(),
            field_before: plan.field_before.lock().clone(),
            app_name: plan.app_name.clone(),
        };
        match smart.format(&request) {
            Some(formatted) => (formatted, true),
            None => (text.to_string(), false),
        }
    }

    #[cfg(not(feature = "smart"))]
    fn format(&self, text: &str, _plan: &DictationPlan) -> (String, bool) {
        (text.to_string(), false)
    }

    #[cfg(feature = "smart")]
    fn load_smart(&mut self, model: WriterModel, models: PathBuf) -> Result<WriterModel, String> {
        self.smart = None;
        kudu_engine::smart::SmartFormatter::load(&model.path(&models), model.short_prompt())
            .map(|formatter| {
                self.smart = Some(formatter);
                model
            })
            .map_err(|e| format!("{e:#}"))
    }

    #[cfg(not(feature = "smart"))]
    fn load_smart(&mut self, _model: WriterModel, _models: PathBuf) -> Result<WriterModel, String> {
        Err("This build of Kudu has no smart formatting.".into())
    }

    fn command(
        &self,
        instruction: String,
        selection: &Arc<Mutex<Option<Option<String>>>>,
        category: AppCategory,
        released: Instant,
        audio_seconds: f64,
    ) -> Result<Finished, String> {
        // The selection is copied while the instruction is transcribed; wait for it briefly.
        let deadline = Instant::now() + Duration::from_millis(1200);
        let selected = loop {
            if let Some(selected) = selection.lock().clone() {
                break selected;
            }
            if Instant::now() > deadline {
                break None;
            }
            std::thread::sleep(Duration::from_millis(15));
        };
        if selected.as_deref().is_none_or(|s| s.trim().is_empty()) && command_output::needs_selection(&instruction) {
            return Err("Select the text first, then hold the Command shortcut".into());
        }
        let text = self.run_command(&instruction, selected.as_deref(), category)?;
        Ok(Finished {
            raw: instruction,
            text,
            formatted: true,
            dictionary_fixes: 0,
            audio_seconds,
            released,
            command: true,
            category,
        })
    }

    #[cfg(feature = "smart")]
    fn run_command(&self, instruction: &str, selection: Option<&str>, category: AppCategory) -> Result<String, String> {
        let smart = self.smart.as_ref().ok_or("Command mode needs the writer model; see Settings → Models")?;
        smart.command(instruction, selection, category).map_err(|e| format!("{e:#}"))
    }

    #[cfg(not(feature = "smart"))]
    fn run_command(&self, _instruction: &str, _selection: Option<&str>, _category: AppCategory) -> Result<String, String> {
        Err("Command mode isn't available in this build".into())
    }
}
