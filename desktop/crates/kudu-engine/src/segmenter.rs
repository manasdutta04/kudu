//! Finds natural pauses in a recording while it's still going, so finished phrases can be
//! transcribed in the background and only the last few seconds are left when the user lets go.
//! That keeps long dictations as quick as short ones on a CPU.

/// Splits audio at pauses. Works on raw samples at any rate.
#[derive(Clone, Debug)]
pub struct Segmenter {
    rate: u32,
    /// Don't cut segments shorter than this: the speech model is more accurate with context.
    pub min_segment: f32,
    /// A pause at least this long is a place to cut.
    pub min_pause: f32,
    /// After this long without a clear pause, a shorter one will do (fast talkers).
    pub max_segment: f32,
}

/// Audio from before a cut that is transcribed again with the next piece, so the words at the
/// boundary get context from both sides (see [`stitch`]).
pub const OVERLAP_SECONDS: f32 = 1.5;

const FRAME_SECONDS: f32 = 0.02;

impl Segmenter {
    pub fn new(rate: u32) -> Self {
        Self { rate, min_segment: 4.0, min_pause: 0.35, max_segment: 10.0 }
    }

    /// Where to cut `samples` after `start` (a sample index), if there's a pause far enough
    /// in: the middle of the first long-enough pause. The audio after the cut stays for later.
    pub fn next_cut(&self, samples: &[f32], start: usize) -> Option<usize> {
        let frame = ((self.rate as f32 * FRAME_SECONDS) as usize).max(1);
        let segment = &samples.get(start..)?;
        let energies: Vec<f32> = segment.chunks_exact(frame).map(rms).collect();
        if energies.len() < 10 {
            return None;
        }
        let threshold = speech_threshold(&energies);
        let min_frames = (self.min_segment / FRAME_SECONDS) as usize;
        let long_frames = (self.max_segment / FRAME_SECONDS) as usize;
        let mut quiet_run = 0usize;
        for (index, energy) in energies.iter().enumerate() {
            let pause_seconds = if index >= long_frames { 0.16 } else { self.min_pause };
            let pause_frames = ((pause_seconds / FRAME_SECONDS) as usize).max(1);
            if *energy < threshold {
                quiet_run += 1;
            } else {
                quiet_run = 0;
            }
            // The pause must have started after the minimum segment length, and the speaker must
            // have said something before it.
            if quiet_run >= pause_frames && index + 1 - quiet_run >= min_frames {
                let pause_start = index + 1 - quiet_run;
                let had_speech = energies[..pause_start].iter().any(|e| *e >= threshold);
                if had_speech {
                    let middle = pause_start + quiet_run / 2;
                    return Some(start + middle * frame);
                }
            }
        }
        None
    }
}

/// Joins a new piece onto the transcript so far, using word times. The piece's audio starts
/// `overlap` seconds before the cut, so its first words were already transcribed at the end of
/// `previous`: those are dropped, and only their punctuation is used, because the new piece
/// heard them together with what came next. A sentence end that was really a pause mid-sentence
/// is undone ("…I also think. We should…" → "…I also think we should…").
pub fn stitch_timed(previous: &str, piece: &[crate::asr::Word], overlap: f32) -> String {
    // A word starting in the last ~80 ms of the overlap is the first new word cut short; keep it.
    let boundary = (overlap - 0.08).max(0.0);
    let (heard_again, new): (Vec<_>, Vec<_>) = piece.iter().partition(|w| w.start < boundary);
    let words: Vec<String> = new.iter().map(|w| w.text.clone()).collect();
    let mut previous = previous.trim().to_string();
    if previous.is_empty() || words.is_empty() {
        let joined = words.join(" ");
        return if previous.is_empty() { joined } else if joined.is_empty() { previous } else { format!("{previous} {joined}") };
    }
    if let Some(last_heard) = heard_again.last() {
        let ends_sentence = |w: &str| w.ends_with(['.', '?', '!']);
        let prev_ends = ends_sentence(&previous);
        let heard_ends = ends_sentence(&last_heard.text) || last_heard.text.ends_with([',', ';', ':']);
        if prev_ends && !heard_ends {
            // Heard with its continuation, it isn't the end of a sentence. (The new words'
            // capitals already come from that same context, so they stay as they are.)
            previous.pop();
        } else if last_heard.text.ends_with(',') && !previous.ends_with(',') && !prev_ends {
            previous.push(',');
        }
    }
    format!("{previous} {}", words.join(" "))
}

/// Joins the transcript of a new piece (which starts with ~1.5 s of audio already transcribed
/// at the end of `previous`) onto `previous`: the re-transcribed boundary words replace the old
/// ones, since they were heard with the words after them ("…I also think. We should…" becomes
/// "…I also think we should…").
pub fn stitch(previous: &str, next: &str) -> String {
    let before: Vec<&str> = previous.split_whitespace().collect();
    let after: Vec<&str> = next.split_whitespace().collect();
    if before.is_empty() {
        return next.trim().to_string();
    }
    if after.is_empty() {
        return previous.trim().to_string();
    }
    let norm = |w: &str| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
    // The overlap is a few words; the new piece may start with a clipped partial word.
    for k in (1..=before.len().min(10)).rev() {
        let tail: Vec<String> = before[before.len() - k..].iter().map(|w| norm(w)).collect();
        for skip in 0..=1usize {
            if after.len() < skip + k {
                continue;
            }
            let head: Vec<String> = after[skip..skip + k].iter().map(|w| norm(w)).collect();
            // Require two words of agreement (or the whole of a short piece) to trust a match.
            if head == tail && (k >= 2 || before.len() == 1) {
                let mut words: Vec<&str> = before[..before.len() - k].to_vec();
                words.extend_from_slice(&after[skip..]);
                return words.join(" ");
            }
        }
    }
    format!("{} {}", previous.trim(), next.trim())
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt()
}

/// Quiet means close to the room's noise floor (the 10th percentile of frame loudness) and
/// well below the speech level (the 90th percentile), with a minimum so a perfectly silent
/// input still has a threshold.
fn speech_threshold(energies: &[f32]) -> f32 {
    let mut sorted = energies.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let floor = sorted[sorted.len() / 10];
    let loud = sorted[sorted.len() * 9 / 10];
    (floor * 3.0).min(loud * 0.25).max(0.004)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 16_000;

    fn speech(seconds: f32) -> Vec<f32> {
        (0..(seconds * RATE as f32) as usize)
            .map(|i| (i as f32 * 0.05).sin() * 0.2 * (1.0 + (i as f32 * 0.0007).sin()) / 2.0 + 0.05)
            .collect()
    }

    fn silence(seconds: f32) -> Vec<f32> {
        (0..(seconds * RATE as f32) as usize).map(|i| ((i * 7919) % 13) as f32 * 0.0001).collect()
    }

    #[test]
    fn cuts_in_the_middle_of_a_pause_after_enough_speech() {
        let mut audio = speech(5.0);
        audio.extend(silence(0.6));
        audio.extend(speech(2.0));
        let cut = Segmenter::new(RATE).next_cut(&audio, 0).expect("a cut");
        let seconds = cut as f32 / RATE as f32;
        assert!((5.1..5.5).contains(&seconds), "cut at {seconds}");
    }

    #[test]
    fn waits_for_the_minimum_segment_length() {
        let mut audio = speech(2.0);
        audio.extend(silence(0.6));
        audio.extend(speech(1.5));
        assert_eq!(Segmenter::new(RATE).next_cut(&audio, 0), None);
    }

    #[test]
    fn ignores_short_breaths() {
        let mut audio = speech(5.0);
        audio.extend(silence(0.15));
        audio.extend(speech(2.0));
        assert_eq!(Segmenter::new(RATE).next_cut(&audio, 0), None);
    }

    #[test]
    fn stitches_the_reheard_boundary_words() {
        assert_eq!(
            stitch("the waitlist I also think.", "also think we should move the call."),
            "the waitlist I also think we should move the call."
        );
        // A clipped first word in the new piece is skipped.
        assert_eq!(stitch("we need to ship it today.", "y it today, so let's go."), "we need to ship it today, so let's go.");
        // No overlap found: plain join.
        assert_eq!(stitch("First part.", "Second part."), "First part. Second part.");
        assert_eq!(stitch("", "Only."), "Only.");
    }

    #[test]
    fn stitches_by_time_and_fixes_the_boundary() {
        use crate::asr::Word;
        let w = |text: &str, start: f32| Word { text: text.into(), start };
        // "…I also think." was cut at a pause; heard again with what follows, it continues.
        let piece = [w("also", 0.4), w("think", 0.9), w("we", 1.6), w("should", 1.8), w("go.", 2.1)];
        assert_eq!(stitch_timed("the waitlist I also think.", &piece, 1.5), "the waitlist I also think we should go.");
        // A real sentence end stays.
        let piece = [w("today.", 0.8), w("Let", 1.7), w("me", 1.9), w("know.", 2.0)];
        assert_eq!(stitch_timed("We ship today.", &piece, 1.5), "We ship today. Let me know.");
        // Names and "I" keep their capitals.
        let piece = [w("think", 0.8), w("Priya", 1.6), w("agrees.", 1.9)];
        assert_eq!(stitch_timed("I think.", &piece, 1.5), "I think Priya agrees.");
        assert_eq!(stitch_timed("", &[w("Only.", 0.0)], 0.0), "Only.");
    }

    #[test]
    fn accepts_a_shorter_pause_in_a_long_stretch() {
        let mut audio = speech(11.0);
        audio.extend(silence(0.2));
        audio.extend(speech(2.0));
        let cut = Segmenter::new(RATE).next_cut(&audio, 0).expect("a cut after a long stretch");
        assert!(cut as f32 / RATE as f32 > 10.9);
    }

    #[test]
    fn continues_from_the_last_cut() {
        let mut audio = speech(5.0);
        audio.extend(silence(0.6));
        audio.extend(speech(5.0));
        audio.extend(silence(0.6));
        let segmenter = Segmenter::new(RATE);
        let first = segmenter.next_cut(&audio, 0).unwrap();
        let second = segmenter.next_cut(&audio, first).unwrap();
        assert!(second as f32 / RATE as f32 > 10.0);
    }
}
