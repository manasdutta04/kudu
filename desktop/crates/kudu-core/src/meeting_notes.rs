//! How meeting notes are written: the prompts for the on-device writer model, reading its
//! reply, and plain rules for when no model is available. The same prompts and rules as the
//! macOS app's `MeetingNotesPrompt`.
//!
//! A long meeting doesn't fit in a small model's context, so the transcript is split into
//! parts; each part is summarised, then the summaries are combined into the notes.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use crate::meeting::{turns, MeetingNotes, MeetingSegment, NotesGenerator};
use crate::sentence_chunker::sentences;
use crate::usage_stats::word_count;

/// Words per part: about 2,000 tokens, leaving room for the instructions and the reply.
pub const PART_WORDS: usize = 1_400;

pub const PART_INSTRUCTIONS: &str = "You summarise one part of a meeting transcript. Lines start with who spoke: \"You\" is \
the person taking notes, \"Others\" is everyone else.\n\
Write short bullet points (\"- \") covering what was discussed, anything decided, and any \
task someone agreed to do (with who and when, if said). Use only what is in the \
transcript. Never invent names, numbers or dates. Output only the bullet points.";

pub const NOTES_INSTRUCTIONS: &str = "You write meeting notes from a transcript or from summaries of its parts. Lines may \
start with who spoke: \"You\" is the person taking notes, \"Others\" is everyone else.\n\
Use only what is in the input. Never invent names, numbers or dates. Be brief and \
concrete. Reply in exactly this format, starting with the TITLE line, and write None \
under a heading with nothing:\n\
TITLE: <what the meeting was about, at most 8 words>\n\
SUMMARY: <two to four sentences>\n\
KEY POINTS:\n\
- <one point per line>\n\
DECISIONS:\n\
- <one decision per line>\n\
ACTION ITEMS:\n\
- <who will do what, and by when if said>";

/// The transcript split into parts of about `max_words` words, at line or sentence ends.
pub fn parts(transcript: &str, max_words: usize) -> Vec<String> {
    let pieces: Vec<String> = transcript
        .lines()
        .flat_map(|line| if word_count(line) > max_words { sentences(line) } else { vec![line.to_string()] })
        .collect();
    let mut parts = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut words = 0;
    for piece in pieces {
        let count = word_count(&piece);
        if words + count > max_words && !current.is_empty() {
            parts.push(current.join("\n"));
            current.clear();
            words = 0;
        }
        current.push(piece);
        words += count;
    }
    if !current.is_empty() {
        parts.push(current.join("\n"));
    }
    parts
}

pub fn part_prompt(part: &str, index: usize, count: usize) -> String {
    format!("Part {} of {count} of the meeting transcript:\n\n{part}", index + 1)
}

pub fn notes_prompt(transcript: &str) -> String {
    format!("Meeting transcript:\n\n{transcript}")
}

pub fn notes_prompt_from_parts(summaries: &[String]) -> String {
    format!("Notes taken during the meeting, from start to end:\n\n{}", summaries.join("\n"))
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Heading {
    Title,
    Summary,
    KeyPoints,
    Decisions,
    Actions,
}

/// Reads a model's reply in the notes format: the title (if any) and the notes. `None` if it
/// isn't in that format.
pub fn parse(reply: &str, generator: NotesGenerator) -> Option<(Option<String>, MeetingNotes)> {
    let mut title: Option<String> = None;
    let mut summary: Vec<String> = Vec::new();
    let mut key_points = Vec::new();
    let mut decisions = Vec::new();
    let mut actions = Vec::new();
    let mut current: Option<Heading> = None;
    let mut headings = 0;

    for raw in reply.lines() {
        let line = raw.trim().trim_matches(|c: char| "#*_ ".contains(c)).trim();
        if line.is_empty() {
            continue;
        }
        if let Some((heading, rest)) = heading_of(line) {
            current = Some(heading);
            headings += 1;
            let rest = clean(rest);
            match heading {
                Heading::Title => title = (!rest.is_empty()).then_some(rest),
                Heading::Summary => {
                    if !rest.is_empty() {
                        summary.push(rest);
                    }
                }
                _ => {
                    if !rest.is_empty() && !is_none(&rest) {
                        list(heading, &mut key_points, &mut decisions, &mut actions).push(rest);
                    }
                }
            }
            continue;
        }
        if current.is_none() && title.is_none() && headings == 0 && word_count(line) <= 12 {
            // A first line before any heading is the title, even without "TITLE:".
            title = Some(clean(line));
            continue;
        }
        match current {
            Some(Heading::Title) => {
                if title.is_none() {
                    title = Some(clean(line));
                }
            }
            Some(Heading::Summary) => summary.push(clean(line)),
            Some(heading) => {
                let item = clean(&bullet(line));
                if !item.is_empty() && !is_none(&item) {
                    list(heading, &mut key_points, &mut decisions, &mut actions).push(item);
                }
            }
            None => {}
        }
    }
    if headings < 2 {
        return None;
    }
    let notes = MeetingNotes {
        summary: summary.join(" ").trim().to_string(),
        key_points: dedupe(key_points),
        decisions: dedupe(decisions),
        actions: dedupe(actions),
        generator,
    };
    if notes.is_empty() {
        return None;
    }
    Some((title.map(|t| t.chars().take(80).collect()), notes))
}

fn list<'a>(heading: Heading, key_points: &'a mut Vec<String>, decisions: &'a mut Vec<String>, actions: &'a mut Vec<String>) -> &'a mut Vec<String> {
    match heading {
        Heading::Decisions => decisions,
        Heading::Actions => actions,
        _ => key_points,
    }
}

fn heading_of(line: &str) -> Option<(Heading, &str)> {
    const TABLE: &[(&str, Heading)] = &[
        ("title", Heading::Title),
        ("summary", Heading::Summary),
        ("key points", Heading::KeyPoints),
        ("decisions", Heading::Decisions),
        ("action items", Heading::Actions),
        ("actions", Heading::Actions),
    ];
    let lower = line.to_lowercase();
    for (name, heading) in TABLE {
        if lower.starts_with(name) && line.is_char_boundary(name.len()) {
            let rest = &line[name.len()..];
            if rest.is_empty() || rest.starts_with(':') {
                return Some((*heading, rest.trim_start_matches([':', '*', ' '])));
            }
        }
    }
    None
}

/// Whether notes from a model look real: something in them, and not just the transcript
/// played back (more than 80% of its length).
pub fn is_usable(notes: &MeetingNotes, transcript_words: usize) -> bool {
    if notes.is_empty() || (notes.summary.is_empty() && notes.key_points.is_empty()) {
        return false;
    }
    notes.word_count() <= ((transcript_words as f64 * 0.8) as usize).max(120)
}

static ACTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(i'll|i will|we'll|we will|you'll|let's|let us|we need to|i need to|you need to|we should|can you|could you|please|action item|follow up|follow-up|next step|next steps|to-do|todo|by (monday|tuesday|wednesday|thursday|friday|saturday|sunday|tomorrow|tonight|next week|the end of))\b").unwrap()
});
static DECISION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(we decided|decided to|we agreed|agreed to|agreed that|let's go with|we're going with|we are going with|we'll go with|final decision|the decision is|settled on)\b").unwrap()
});

/// Notes from plain rules, for when no language model is available: sentences that sound like
/// a task or a decision, and the longest other sentences as key points.
pub fn extractive(segments: &[MeetingSegment]) -> MeetingNotes {
    let sentences: Vec<String> = turns(segments)
        .iter()
        .flat_map(|turn| sentences(&turn.text))
        .map(|s| s.trim().to_string())
        .filter(|s| word_count(s) >= 4)
        .collect();
    let actions: Vec<String> = dedupe(sentences.iter().filter(|s| ACTION.is_match(s)).cloned().collect()).into_iter().take(8).collect();
    let decisions: Vec<String> = dedupe(sentences.iter().filter(|s| DECISION.is_match(s) && !actions.contains(s)).cloned().collect())
        .into_iter()
        .take(5)
        .collect();
    let taken: HashSet<&String> = actions.iter().chain(&decisions).collect();
    let mut longest: Vec<(usize, &String)> =
        sentences.iter().enumerate().filter(|(_, s)| !taken.contains(s) && word_count(s) >= 8).collect();
    longest.sort_by_key(|(_, s)| std::cmp::Reverse(word_count(s)));
    longest.truncate(5);
    longest.sort_by_key(|(index, _)| *index);
    let key_points = dedupe(longest.into_iter().map(|(_, s)| s.clone()).collect());
    let summary = sentences.iter().filter(|s| word_count(s) >= 6).take(2).cloned().collect::<Vec<_>>().join(" ");
    MeetingNotes { summary, key_points, decisions, actions, generator: NotesGenerator::Extractive }
}

fn bullet(line: &str) -> String {
    let mut text = line;
    if let Some(rest) = text.strip_prefix(['-', '•', '*', '–']) {
        text = rest;
    } else if let Some(dot) = text.find(['.', ')']) {
        if dot > 0 && text[..dot].chars().all(|c| c.is_ascii_digit()) {
            text = &text[dot + 1..];
        }
    }
    let trimmed = text.trim_start();
    trimmed.strip_prefix("[ ]").unwrap_or(trimmed).to_string()
}

fn clean(text: &str) -> String {
    text.trim_matches(|c: char| c == '*' || c == '_' || c == '"' || c.is_whitespace()).to_string()
}

fn is_none(text: &str) -> bool {
    let lower = text.to_lowercase();
    let lower = lower.trim_matches(['.', ' ']);
    ["none", "n/a", "no decisions", "no action items", "nothing", "none mentioned", "none were made"].contains(&lower)
}

fn dedupe(items: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    items.into_iter().filter(|item| seen.insert(item.to_lowercase())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meeting::AudioSource;

    const REPLY: &str = "TITLE: Launch plan review
SUMMARY: The team reviewed the launch plan. The date moves to next Tuesday.
KEY POINTS:
- The review found two blocking bugs
- Marketing is ready
DECISIONS:
- Launch moves to Tuesday
ACTION ITEMS:
- Priya will fix the login bug by Friday
- You: send the deck to Sakshi";

    #[test]
    fn reads_the_notes_format() {
        let (title, notes) = parse(REPLY, NotesGenerator::KuduWriter).unwrap();
        assert_eq!(title.as_deref(), Some("Launch plan review"));
        assert_eq!(notes.summary, "The team reviewed the launch plan. The date moves to next Tuesday.");
        assert_eq!(notes.key_points, vec!["The review found two blocking bugs", "Marketing is ready"]);
        assert_eq!(notes.decisions, vec!["Launch moves to Tuesday"]);
        assert_eq!(notes.actions.len(), 2);
    }

    #[test]
    fn tolerates_markdown_and_none() {
        let reply = "**Title:** Weekly sync\n## Summary\nQuick status round.\n## Key points\n1. Hiring is on track\n* Budget approved\n## Decisions\nNone\n## Action items\n- [ ] Manas books the room";
        let (title, notes) = parse(reply, NotesGenerator::KuduWriter).unwrap();
        assert_eq!(title.as_deref(), Some("Weekly sync"));
        assert_eq!(notes.summary, "Quick status round.");
        assert_eq!(notes.key_points, vec!["Hiring is on track", "Budget approved"]);
        assert!(notes.decisions.is_empty());
        assert_eq!(notes.actions, vec!["Manas books the room"]);
    }

    #[test]
    fn a_first_line_without_a_heading_is_the_title() {
        let (title, _) = parse("Budget review\nSUMMARY: Short.\nKEY POINTS:\n- One", NotesGenerator::KuduWriter).unwrap();
        assert_eq!(title.as_deref(), Some("Budget review"));
    }

    #[test]
    fn rejects_a_reply_that_isnt_notes() {
        assert!(parse("Sure! Here is what they talked about: the launch.", NotesGenerator::KuduWriter).is_none());
    }

    #[test]
    fn rejects_the_transcript_played_back() {
        let long = MeetingNotes { summary: "word ".repeat(400), key_points: vec![], decisions: vec![], actions: vec![], generator: NotesGenerator::KuduWriter };
        assert!(!is_usable(&long, 300));
        let short = MeetingNotes { summary: "A short summary.".into(), key_points: vec!["One".into()], decisions: vec![], actions: vec![], generator: NotesGenerator::KuduWriter };
        assert!(is_usable(&short, 300));
    }

    #[test]
    fn splits_long_transcripts_at_lines() {
        let line = format!("Others: {}", "word ".repeat(100).trim());
        let transcript = vec![line; 30].join("\n");
        let split = parts(&transcript, PART_WORDS);
        assert_eq!(split.len(), 3);
        assert!(split.iter().all(|p| word_count(p) <= PART_WORDS));
        assert_eq!(parts("You: short", PART_WORDS).len(), 1);
    }

    #[test]
    fn extractive_notes_find_tasks_and_decisions() {
        let segments = vec![
            MeetingSegment { start: 0.0, end: 10.0, source: AudioSource::System, text: "Thanks for joining the planning call today everyone. We looked at the numbers from last quarter and they were better than expected across regions.".into() },
            MeetingSegment { start: 10.0, end: 20.0, source: AudioSource::Mic, text: "We agreed to launch on Tuesday next week. I'll send the deck to Sakshi by Friday.".into() },
            MeetingSegment { start: 20.0, end: 25.0, source: AudioSource::System, text: "Okay. Can you also update the pricing page?".into() },
        ];
        let notes = extractive(&segments);
        assert_eq!(notes.generator, NotesGenerator::Extractive);
        assert_eq!(notes.decisions, vec!["We agreed to launch on Tuesday next week."]);
        assert_eq!(notes.actions, vec!["I'll send the deck to Sakshi by Friday.", "Can you also update the pricing page?"]);
        assert!(!notes.summary.is_empty());
        assert!(!notes.key_points.is_empty());
    }
}
