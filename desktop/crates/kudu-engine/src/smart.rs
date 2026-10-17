//! Smart formatting and Command mode with the small local language model. The same
//! instructions and the same safety checks as the Mac app: the model may punctuate, lay out
//! lists and write numbers, but `formatting_guard` rejects any output that changes the words,
//! answers the text or drops part of it, and then the rule-cleaned text is used instead.

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Result};
use kudu_core::{command_output, formatting_guard, word_count, AppCategory};

use crate::llm::SmallModel;
pub use crate::prompts::FormatRequest;
use crate::prompts::{destination, system_prompt, user_prompt, COMMAND_INSTRUCTIONS};

pub struct SmartFormatter {
    model: SmallModel,
    /// Use the short formatting prompt (contract v2), for writers trained on it.
    short_prompt: bool,
}

impl SmartFormatter {
    /// Loads a writer model. `short_prompt` selects the brief formatting instructions that
    /// Kudu's own writers were trained on; Command mode always uses its full instructions.
    pub fn load(path: &Path, short_prompt: bool) -> Result<Self> {
        let formatter = Self { model: SmallModel::load(path)?, short_prompt };
        // Warm up: the first reply pays for loading weights into the cache.
        let _ = formatter.model.chat("Reply with OK.", "OK", 2, Duration::from_secs(5));
        Ok(formatter)
    }

    /// The formatted text, or `None` to keep the input (model too slow, or it changed the words).
    pub fn format(&self, request: &FormatRequest) -> Option<String> {
        let words = word_count(&request.text);
        // Generation time grows with length; a CPU needs more room than Apple's model.
        let budget = Duration::from_millis((2500 + 40 * words as u64).min(8000));
        let max_tokens = (words * 2 + 48).min(1024);
        let code = request.category == AppCategory::Code;
        let instructions = system_prompt(request.category, self.short_prompt);
        let reply = match self.model.chat(instructions, &user_prompt(request), max_tokens, budget) {
            Ok(reply) => reply,
            Err(error) => {
                log::info!("Formatting skipped: {error}");
                return None;
            }
        };
        // Drop any instruction line the model echoed back.
        let output: String = reply
            .lines()
            .filter(|line| {
                let lower = line.to_lowercase();
                !line.starts_with("Note:") && !lower.starts_with("it will be typed into") && !lower.starts_with("this is a to-do app")
            })
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
        let output = output.strip_prefix("Formatted:").map(str::trim).unwrap_or(&output).to_string();
        let accepted = if code {
            formatting_guard::accept_code(&request.text, &output)
        } else {
            formatting_guard::accept(&request.text, &output, request.strength)
        };
        if accepted.is_none() {
            log::info!("Formatting rejected: the output changed the words");
            if std::env::var_os("KUDU_DEBUG_FORMAT").is_some() {
                eprintln!("rejected model output:\n{output}");
            }
        }
        accepted
    }

    /// One free-form turn with the writer (meeting notes): `system` instructions and the `user`
    /// message, up to `max_tokens` and `timeout`.
    pub fn chat(&self, system: &str, user: &str, max_tokens: usize, timeout: Duration) -> Result<String> {
        self.model.chat(system, user, max_tokens, timeout)
    }

    /// Command mode: applies a spoken instruction to the selected text, or drafts new text when
    /// nothing is selected.
    pub fn command(&self, instruction: &str, selection: Option<&str>, category: AppCategory) -> Result<String> {
        let prompt = match selection.filter(|s| !s.trim().is_empty()) {
            Some(selection) => match translation_target(instruction) {
                // Small models follow a plain translation request far more reliably.
                Some(language) => format!(
                    "Translate this text into {language}. Write it in {language} only and output only the translation.\n\nText:\n{selection}"
                ),
                None => format!(
                    "Rewrite the text below following this instruction: {instruction}\nReturn only the rewritten text.\n\nText:\n{selection}"
                ),
            },
            None => [format!("Request: {instruction}"), destination(category).to_string()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n"),
        };
        let input_words = word_count(selection.unwrap_or("")) + word_count(instruction);
        let max_tokens = (input_words * 3 + 200).min(1500);
        let reply = self.model.chat(COMMAND_INSTRUCTIONS, &prompt, max_tokens, Duration::from_secs(20))?;
        let text = command_output::clean(&reply, Some(instruction));
        if is_refusal(&text, instruction) {
            return Err(anyhow!("The model couldn't do that"));
        }
        if let Some(language) = translation_target(instruction) {
            if !in_script_of(&text, &language) {
                return Err(anyhow!("The small model can't translate into {language} well"));
            }
        }
        if text.is_empty() {
            return Err(anyhow!("No result"));
        }
        Ok(text)
    }
}

/// "translate to Hindi", "translate this into french" → "Hindi", "French".
fn translation_target(instruction: &str) -> Option<String> {
    let lower = instruction.to_lowercase();
    let rest = lower.split_once("translate")?.1;
    let after = rest.split_once(" to ").or_else(|| rest.split_once(" into "))?.1;
    let language: String = after.trim().trim_end_matches(['.', '!', '?']).split_whitespace().next()?.to_string();
    let mut chars = language.chars();
    Some(chars.next()?.to_uppercase().chain(chars).collect())
}

/// Whether a translation is written in the target language's script (small models sometimes
/// answer in another language altogether). Unknown languages pass.
fn in_script_of(text: &str, language: &str) -> bool {
    let range: Option<std::ops::RangeInclusive<u32>> = match language.to_lowercase().as_str() {
        "hindi" | "marathi" | "nepali" | "sanskrit" => Some(0x0900..=0x097F),
        "bengali" | "bangla" | "assamese" => Some(0x0980..=0x09FF),
        "punjabi" => Some(0x0A00..=0x0A7F),
        "gujarati" => Some(0x0A80..=0x0AFF),
        "tamil" => Some(0x0B80..=0x0BFF),
        "telugu" => Some(0x0C00..=0x0C7F),
        "kannada" => Some(0x0C80..=0x0CFF),
        "malayalam" => Some(0x0D00..=0x0D7F),
        "arabic" | "urdu" | "persian" | "farsi" => Some(0x0600..=0x06FF),
        "russian" | "ukrainian" | "bulgarian" => Some(0x0400..=0x04FF),
        "greek" => Some(0x0370..=0x03FF),
        "hebrew" => Some(0x0590..=0x05FF),
        "korean" => Some(0xAC00..=0xD7AF),
        "japanese" => Some(0x3040..=0x30FF),
        "chinese" | "mandarin" => Some(0x4E00..=0x9FFF),
        _ => None,
    };
    let Some(range) = range else { return true };
    let letters: Vec<u32> = text.chars().filter(|c| c.is_alphabetic()).map(|c| c as u32).collect();
    let matching = letters.iter().filter(|c| range.contains(c)).count();
    !letters.is_empty() && matching * 2 >= letters.len()
}

/// "I'm sorry, but I can't…": small models sometimes refuse harmless edits; never paste that.
/// An apology the user asked for ("make it sound apologetic") is the answer, not a refusal,
/// unless it also says it can't do something.
fn is_refusal(text: &str, instruction: &str) -> bool {
    let lower = text.trim_start().to_lowercase();
    if ["i can't assist", "i cannot assist", "i can't help", "as an ai"].iter().any(|p| lower.starts_with(p)) {
        return true;
    }
    if !["i'm sorry", "i am sorry", "sorry, but"].iter().any(|p| lower.starts_with(p)) {
        return false;
    }
    let asked = instruction.to_lowercase();
    let wants_apology = asked.contains("apolog") || asked.contains("sorry");
    let declines = ["can't", "cannot", "unable", "not able"].iter().any(|p| lower.contains(p));
    !wants_apology || declines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn understands_translation_requests() {
        assert_eq!(translation_target("translate to Hindi").as_deref(), Some("Hindi"));
        assert_eq!(translation_target("Translate this into french.").as_deref(), Some("French"));
        assert_eq!(translation_target("make it shorter"), None);
    }

    #[test]
    fn an_apology_that_was_asked_for_is_not_a_refusal() {
        let apology = "I'm sorry, but the delivery will be late by 2 days.";
        assert!(!is_refusal(apology, "make it sound apologetic"));
        assert!(is_refusal(apology, "make it shorter"));
        assert!(is_refusal("I'm sorry, but I can't rewrite that.", "make it sound apologetic"));
        assert!(is_refusal("As an AI, I can't do that.", "say sorry"));
        assert!(!is_refusal("The delivery will be late by 2 days.", "make it shorter"));
    }

    #[test]
    fn short_prompts_are_much_shorter() {
        use crate::prompts::{PROSE_INSTRUCTIONS, PROSE_INSTRUCTIONS_SHORT};
        assert_eq!(system_prompt(AppCategory::General, true), PROSE_INSTRUCTIONS_SHORT);
        assert_eq!(system_prompt(AppCategory::Email, false), PROSE_INSTRUCTIONS);
        assert!(PROSE_INSTRUCTIONS_SHORT.len() * 4 < PROSE_INSTRUCTIONS.len());
    }

    /// kudu-writer-tiny, downloaded once (checksum verified) for the tests that need it. The
    /// tests run in parallel, so the download is guarded: two of them writing the same file at
    /// once would trip over each other.
    fn test_writer() -> SmartFormatter {
        use crate::models::{install_writer, WriterModel};
        static DOWNLOAD: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let dir = std::env::temp_dir().join("kudu-writer-test");
        let model = WriterModel::Tiny;
        {
            let _guard = DOWNLOAD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if !model.is_installed(&dir) {
                install_writer(model, &dir, &mut |_| {}, &|| false).expect("download");
            }
        }
        SmartFormatter::load(&model.path(&dir), model.short_prompt()).expect("load")
    }

    /// Downloads kudu-writer-tiny (checksum verified), formats a spoken list with the short
    /// prompt, and checks that the guard keeps the result. Needs the network and ~230 MB, so it
    /// runs in CI (`--include-ignored`) only.
    #[test]
    #[ignore = "downloads a writer model"]
    fn kudu_writer_formats_a_spoken_list() {
        let formatter = test_writer();
        let request = FormatRequest {
            text: "quick update three things first design is done second backend is waiting on auth third we ship on monday"
                .into(),
            ..Default::default()
        };
        // Called without the app's time budget: CI machines are slower than a laptop.
        let reply = formatter
            .model
            .chat(system_prompt(request.category, true), &user_prompt(&request), 200, Duration::from_secs(120))
            .expect("reply");
        let kept = formatting_guard::accept(&request.text, &reply, request.strength);
        assert!(kept.is_some(), "the guard rejected: {reply}");
        assert!(reply.contains('•') || reply.contains("1."), "not a list: {reply}");
    }

    /// Has kudu-writer-tiny write meeting notes for a short transcript and prints what came
    /// back, and whether Kudu could use it (otherwise the app falls back to its rule-based
    /// notes). Informational: a small formatting model may not manage it. CI only.
    #[test]
    #[ignore = "downloads a writer model"]
    fn kudu_writer_writes_meeting_notes() {
        use kudu_core::meeting_notes;
        let formatter = test_writer();
        let transcript = "Others: Hi everyone, thanks for joining. Today we need to settle the launch date for the new billing page. The review found two blocking bugs in the checkout flow.\n\
You: Thanks. I looked at both bugs this morning. The login timeout one is easy, I'll fix it by Friday. The tax rounding bug needs help from the payments team.\n\
Others: Okay. Then let's move the launch to next Tuesday instead of Thursday. Can you also send the updated deck to Sakshi before the call with finance?\n\
You: Yes, we agreed on Tuesday. I will send the deck to Sakshi tomorrow and ask Priya from payments to look at the rounding bug.";
        let start = std::time::Instant::now();
        let reply = formatter
            .chat(meeting_notes::NOTES_INSTRUCTIONS, &meeting_notes::notes_prompt(transcript), 520, Duration::from_secs(300))
            .expect("reply");
        let parsed = meeting_notes::parse(&reply, kudu_core::meeting::NotesGenerator::KuduWriter);
        let usable = parsed.as_ref().is_some_and(|(_, notes)| meeting_notes::is_usable(notes, kudu_core::word_count(transcript)));
        println!("Meeting notes in {:.1} s, usable: {usable}\n{reply}", start.elapsed().as_secs_f32());
        assert!(!reply.trim().is_empty());
    }

    #[test]
    fn checks_the_script_of_a_translation() {
        assert!(in_script_of("कल ऑफिस में मिलते हैं।", "Hindi"));
        assert!(!in_script_of("相见明天办公室。", "Hindi"));
        assert!(in_script_of("Hasta mañana en la oficina.", "Spanish"));
    }
}
