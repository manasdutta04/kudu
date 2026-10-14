//! On-device speech recognition with sherpa-onnx (ONNX Runtime, CPU).

use std::path::Path;
use std::time::Instant;

use anyhow::{anyhow, Result};
use sherpa_onnx::{OfflineMoonshineModelConfig, OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig};

use crate::audio::SAMPLE_RATE;
use crate::models::SpeechModel;

/// 200 ms of silence around each clip. Transducer models tend to drop or garble the first and
/// last word when speech starts or ends right at the edge of the audio.
const PADDING: usize = (SAMPLE_RATE as usize) / 5;

pub struct SpeechEngine {
    recognizer: OfflineRecognizer,
    model: SpeechModel,
}

impl SpeechEngine {
    /// Loads a downloaded model. Takes a second or two; do it off the UI path, once.
    pub fn load(model: SpeechModel, models_dir: &Path) -> Result<Self> {
        let dir = model.dir(models_dir);
        let path = |file: &str| Some(dir.join(file).to_string_lossy().into_owned());
        let mut config = OfflineRecognizerConfig::default();
        match model {
            SpeechModel::Parakeet => {
                config.model_config.transducer = OfflineTransducerModelConfig {
                    encoder: path("encoder.int8.onnx"),
                    decoder: path("decoder.int8.onnx"),
                    joiner: path("joiner.int8.onnx"),
                };
                config.model_config.model_type = Some("nemo_transducer".into());
            }
            SpeechModel::MoonshineBase => {
                config.model_config.moonshine = OfflineMoonshineModelConfig {
                    preprocessor: path("preprocess.onnx"),
                    encoder: path("encode.int8.onnx"),
                    uncached_decoder: path("uncached_decode.int8.onnx"),
                    cached_decoder: path("cached_decode.int8.onnx"),
                    ..Default::default()
                };
            }
        }
        config.model_config.tokens = path("tokens.txt");
        config.model_config.num_threads = threads();
        config.model_config.provider = Some("cpu".into());
        config.decoding_method = Some("greedy_search".into());

        let start = Instant::now();
        let recognizer = OfflineRecognizer::create(&config).ok_or_else(|| anyhow!("Couldn't load the speech model"))?;
        let engine = Self { recognizer, model };
        // The first decode allocates and tunes kernels; do it now, not on the first dictation.
        engine.transcribe(&vec![0.0; SAMPLE_RATE as usize]);
        log::info!("Speech model {:?} ready in {} ms", model, start.elapsed().as_millis());
        Ok(engine)
    }

    pub fn model(&self) -> SpeechModel {
        self.model
    }

    /// Transcribes 16 kHz mono audio. Returns the text with the model's own punctuation and
    /// capitals, trimmed.
    pub fn transcribe(&self, samples: &[f32]) -> String {
        self.decode(samples).map(|r| r.text.trim().to_string()).unwrap_or_default()
    }

    /// Transcribes and returns the words with their start times (seconds from the start of
    /// `samples`), or `None` when the model doesn't report times.
    pub fn transcribe_words(&self, samples: &[f32]) -> (String, Option<Vec<Word>>) {
        let Some(result) = self.decode(samples) else { return (String::new(), Some(Vec::new())) };
        let text = result.text.trim().to_string();
        let words = result.timestamps.as_ref().and_then(|times| words(&result.tokens, times));
        (text, words)
    }

    fn decode(&self, samples: &[f32]) -> Option<sherpa_onnx::OfflineRecognizerResult> {
        let mut padded = Vec::with_capacity(samples.len() + 2 * PADDING);
        padded.resize(PADDING, 0.0);
        padded.extend_from_slice(samples);
        padded.resize(padded.len() + PADDING, 0.0);

        let stream = self.recognizer.create_stream();
        stream.accept_waveform(SAMPLE_RATE as i32, &padded);
        self.recognizer.decode(&stream);
        stream.get_result()
    }
}

/// A recognized word and when it starts.
#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    pub text: String,
    pub start: f32,
}

/// Groups sub-word tokens into words. A "▁" (SentencePiece) or space marks the start of a
/// word — sometimes as a token of its own, before digits — and closing punctuation attaches to
/// the word before.
fn words(tokens: &[String], times: &[f32]) -> Option<Vec<Word>> {
    if tokens.len() != times.len() {
        return None;
    }
    let padding = PADDING as f32 / SAMPLE_RATE as f32;
    let mut words: Vec<Word> = Vec::new();
    let mut new_word = true;
    for (token, time) in tokens.iter().zip(times) {
        if token.starts_with('\u{2581}') || token.starts_with(' ') {
            new_word = true;
        }
        let piece = token.trim_start_matches(['\u{2581}', ' ']);
        if piece.is_empty() {
            continue;
        }
        let closing = piece.starts_with(['.', ',', '?', '!', ';', ':', ')', '\'', '’']);
        match words.last_mut() {
            Some(last) if !new_word || closing => last.text.push_str(piece),
            _ => words.push(Word { text: piece.to_string(), start: (time - padding).max(0.0) }),
        }
        new_word = false;
    }
    Some(words)
}

/// Speech recognition is compute-bound: use the performance cores, but leave room for the app
/// the user is typing into.
fn threads() -> i32 {
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    cores.saturating_sub(2).clamp(2, 8) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_tokens_into_timed_words() {
        let tokens: Vec<String> = ["▁He", "y", ",", "▁can", "▁we", "▁move", "▁it", "?"].iter().map(|s| s.to_string()).collect();
        let times = [0.3, 0.36, 0.4, 0.5, 0.7, 0.9, 1.1, 1.2];
        let grouped = words(&tokens, &times).unwrap();
        let texts: Vec<&str> = grouped.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(texts, ["Hey,", "can", "we", "move", "it?"]);
        assert!((grouped[0].start - 0.1).abs() < 1e-6, "padding removed");

        // A lone "▁" before digits still starts a new word.
        let tokens: Vec<String> = ["▁at", "▁", "3", ".", "30", "▁up", "dated", "▁", "version"].iter().map(|s| s.to_string()).collect();
        let grouped = words(&tokens, &[0.0, 0.1, 0.12, 0.14, 0.16, 0.3, 0.35, 0.4, 0.42]).unwrap();
        let texts: Vec<&str> = grouped.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(texts, ["at", "3.30", "updated", "version"]);
    }
}
