//! Transcribes WAV files with a downloaded speech model, downloading it first if needed.
//!
//! cargo run --release -p kudu-engine --example transcribe -- <models-dir> [parakeet|moonshine] file.wav...

use std::path::PathBuf;
use std::time::Instant;

use kudu_engine::models::{install_speech_model, SpeechModel};
use kudu_engine::resample::resample;
use kudu_engine::{SpeechEngine, SAMPLE_RATE};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let models = PathBuf::from(args.next().expect("models directory"));
    let model = match args.next().as_deref() {
        Some("moonshine") => SpeechModel::MoonshineBase,
        _ => SpeechModel::Parakeet,
    };
    if !model.is_installed(&models) {
        eprintln!("Downloading {:?}…", model);
        let mut last = 0;
        install_speech_model(model, &models, &mut |f| {
            let pct = (f * 100.0) as i32;
            if pct >= last + 10 {
                last = pct;
                eprintln!("  {pct}%");
            }
        }, &|| false)?;
    }
    let engine = SpeechEngine::load(model, &models)?;
    for file in args {
        let (samples, rate) = read_wav(&file)?;
        let audio = resample(&samples, rate, SAMPLE_RATE);
        let start = Instant::now();
        let text = engine.transcribe(&audio);
        println!(
            "{file}: {:.1}s audio → {} ms\n  {text}",
            audio.len() as f32 / SAMPLE_RATE as f32,
            start.elapsed().as_millis()
        );
        // As during a live dictation: cut at pauses, transcribe the pieces, time only the tail.
        use kudu_engine::segmenter::{stitch, stitch_timed, Segmenter, OVERLAP_SECONDS};
        let segmenter = Segmenter::new(rate);
        let overlap = (OVERLAP_SECONDS * rate as f32) as usize;
        let (mut from, mut text, mut pieces) = (0usize, String::new(), 0);
        let piece = |text: &str, from: usize, to: usize| {
            let start = if from == 0 { 0 } else { from.saturating_sub(overlap) };
            let (plain, words) = engine.transcribe_words(&resample(&samples[start..to], rate, SAMPLE_RATE));
            let heard_overlap = (from - start) as f32 / rate as f32;
            match words {
                Some(words) => stitch_timed(text, &words, heard_overlap),
                None => stitch(text, &plain),
            }
        };
        while let Some(cut) = segmenter.next_cut(&samples, from) {
            text = piece(&text, from, cut);
            from = cut;
            pieces += 1;
        }
        let tail_start = Instant::now();
        text = piece(&text, from, samples.len());
        println!("  segmented ({} pieces, tail {} ms): {text}", pieces + 1, tail_start.elapsed().as_millis());
    }
    Ok(())
}

/// Minimal reader for 16-bit PCM and 32-bit float WAV files (mono or multi-channel).
fn read_wav(path: &str) -> anyhow::Result<(Vec<f32>, u32)> {
    let data = std::fs::read(path)?;
    anyhow::ensure!(&data[0..4] == b"RIFF" && &data[8..12] == b"WAVE", "not a WAV file");
    let (mut pos, mut format, mut channels, mut rate, mut bits) = (12, 0u16, 1u16, 16_000u32, 16u16);
    while pos + 8 <= data.len() {
        let id = &data[pos..pos + 4];
        let size = u32::from_le_bytes(data[pos + 4..pos + 8].try_into()?) as usize;
        let body = &data[pos + 8..(pos + 8 + size).min(data.len())];
        if id == b"fmt " {
            format = u16::from_le_bytes(body[0..2].try_into()?);
            channels = u16::from_le_bytes(body[2..4].try_into()?);
            rate = u32::from_le_bytes(body[4..8].try_into()?);
            bits = u16::from_le_bytes(body[14..16].try_into()?);
        } else if id == b"data" {
            let frames: Vec<f32> = match (format, bits) {
                (1, 16) => body.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b) as f32 / 32768.0).collect(),
                (3, 32) => body.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect(),
                _ => anyhow::bail!("unsupported WAV format {format}/{bits}"),
            };
            let mono = frames
                .chunks(channels as usize)
                .map(|c| c.iter().sum::<f32>() / c.len() as f32)
                .collect();
            return Ok((mono, rate));
        }
        pos += 8 + size + (size & 1);
    }
    anyhow::bail!("no audio data")
}
