//! Learns from corrections: after a dictation, watches the field Kudu typed into (through the
//! platform's accessibility API) for up to a minute. If the user fixes a word Kudu got wrong,
//! the fix is learned as soon as the edit settles, and saved to the dictionary. Fields that
//! don't expose their text are skipped. Nothing is stored except the learned word.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use kudu_core::{correction_learner, LearnedCorrection};

use super::vocabulary;
use crate::platform::Platform;

const DURATION: Duration = Duration::from_secs(60);
const INTERVAL: Duration = Duration::from_millis(1000);

/// Starts and cancels watches; only the latest one runs.
#[derive(Clone, Default)]
pub struct EditObserver {
    generation: Arc<AtomicU64>,
}

impl EditObserver {
    pub fn cancel(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    /// Call right after inserting `inserted`. `known` are spellings already in the dictionary.
    pub fn watch(&self, platform: Arc<dyn Platform>, inserted: String, known: Vec<String>, learned: Sender<Vec<LearnedCorrection>>) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let current = self.generation.clone();
        let inserted = inserted.trim().to_string();
        if inserted.split_whitespace().count() < 1 {
            return;
        }
        let app = platform.focused_app().map(|a| a.id);
        std::thread::Builder::new()
            .name("kudu-learning".into())
            .spawn(move || {
                let alive = || current.load(Ordering::SeqCst) == generation;
                // Give the app a moment to apply the paste; web views can take a while.
                let mut snapshot = None;
                for delay in [400u64, 600, 1000] {
                    std::thread::sleep(Duration::from_millis(delay));
                    if !alive() {
                        return;
                    }
                    if let Some(value) = platform.focused_field_text() {
                        if let Some(range) = locate(&inserted, &value) {
                            snapshot = Some((value[..range.0].to_string(), value[range.1..].to_string()));
                            break;
                        }
                    }
                }
                let Some((prefix, suffix)) = snapshot else {
                    log::debug!("Learning: the field's text isn't readable here");
                    return;
                };
                let learn = |edited: &str| correction_learner::learn(&inserted, edited, &known, vocabulary::is_known_word);
                let deadline = Instant::now() + DURATION;
                let mut latest: Option<String> = None;
                let mut stable = 0;
                while Instant::now() < deadline && alive() {
                    std::thread::sleep(INTERVAL);
                    if platform.focused_app().map(|a| a.id) != app {
                        break; // the user switched apps
                    }
                    let Some(current) = platform.focused_field_text() else { break };
                    if current.len() < prefix.len() + suffix.len() || !current.starts_with(&prefix) || !current.ends_with(&suffix) {
                        break; // sent, cleared, or the text around it changed
                    }
                    let edited = current[prefix.len()..current.len() - suffix.len()].trim().to_string();
                    let edit = (edited != inserted).then_some(edited);
                    if edit == latest {
                        stable += 1;
                    } else {
                        latest = edit;
                        stable = 0;
                    }
                    // The edit has settled: learn now, so the notice comes while it's relevant.
                    if let Some(edited) = latest.as_deref().filter(|_| stable >= 2) {
                        let corrections = learn(edited);
                        if !corrections.is_empty() {
                            let _ = learned.send(corrections);
                            return;
                        }
                    }
                }
                if let Some(edited) = latest.filter(|_| alive()) {
                    let corrections = learn(&edited);
                    if !corrections.is_empty() {
                        let _ = learned.send(corrections);
                    }
                }
            })
            .ok();
    }
}

/// Where Kudu's text sits in the field (byte range). Web views may turn spaces into
/// non-breaking ones, so whitespace matches loosely.
fn locate(inserted: &str, text: &str) -> Option<(usize, usize)> {
    if let Some(start) = text.rfind(inserted) {
        return Some((start, start + inserted.len()));
    }
    let words: Vec<String> = inserted.split_whitespace().map(regex::escape).collect();
    let pattern = regex::Regex::new(&words.join(r"[\s\u{00a0}\u{200b}]+")).ok()?;
    pattern.find_iter(text).last().map(|m| (m.start(), m.end()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_inserted_text_even_with_other_spaces() {
        assert_eq!(locate("the hitmap", "So the hitmap looks good"), Some((3, 13)));
        let field = "So the\u{00a0}hitmap looks";
        let (start, end) = locate("the hitmap", field).unwrap();
        assert_eq!(&field[start..end], "the\u{00a0}hitmap");
        assert_eq!(locate("missing", "nothing here"), None);
    }
}
